//! StatusNotifierItem backend. All D-Bus work stays on a worker thread; only commands
//! and profile snapshots cross the boundary to GPUI.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
};
use std::time::Duration;

use colima_gpui::{
    app::{self, APP_NAME},
    settings,
    store::{Op, ProfileStore},
};
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task};
use ksni::{blocking::TrayMethods, menu::StandardItem};

use super::{
    icon,
    model::{self, Command, Row, State},
};

const POLL: Duration = Duration::from_millis(100);
struct GlobalTray(Entity<Tray>);
impl Global for GlobalTray {}

struct Tray {
    worker: Option<Worker>,
    rows: Vec<Row>,
    _subscriptions: Vec<Subscription>,
    _events: Task<()>,
}

struct Worker {
    rows: Sender<Vec<Row>>,
    commands: Receiver<Command>,
    ready: Arc<AtomicBool>,
    online: Arc<AtomicBool>,
}

pub fn init(cx: &mut App) {
    let tray = cx.new(Tray::new);
    cx.set_global(GlobalTray(tray));
}

pub fn keeps_running(cx: &App) -> bool {
    settings::get(cx).keep_running_in_menu_bar
        && cx.try_global::<GlobalTray>().is_some_and(|tray| {
            tray.0
                .read(cx)
                .worker
                .as_ref()
                .is_some_and(Worker::available)
        })
}

// Linux has no Dock activation policy to change.
pub fn enter_background(_: &mut App) {}

impl Tray {
    fn new(cx: &mut Context<Self>) -> Self {
        let store = ProfileStore::global(cx);
        let subscriptions = vec![
            cx.observe(&store, |this, _, cx| this.sync(cx)),
            cx.observe_global::<settings::Settings>(|this, cx| this.sync(cx)),
        ];
        let events = cx.spawn(async move |this, cx| {
            loop {
                if this
                    .update(cx, |this, cx| {
                        if let Some(worker) = &this.worker {
                            let commands: Vec<_> = worker.commands.try_iter().collect();
                            for command in commands {
                                run(command, cx);
                            }
                            // If the host goes away while hidden, immediately restore a window.
                            if !worker.available() && cx.windows().is_empty() {
                                show_window(None, cx);
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(POLL).await;
            }
        });
        let mut tray = Self {
            worker: None,
            rows: Vec::new(),
            _subscriptions: subscriptions,
            _events: events,
        };
        tray.sync(cx);
        tray
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        let store = ProfileStore::global(cx).read(cx);
        let rows = model::rows(
            store.instances(),
            |name| store.busy(name),
            |name| store.error(name).is_some(),
        );
        if !settings::get(cx).keep_running_in_menu_bar {
            // Disconnecting the snapshot channel shuts the service down on its worker.
            self.worker = None;
            if cx.windows().is_empty() {
                show_window(None, cx);
            }
        } else if let Some(worker) = &self.worker {
            if rows != self.rows {
                let _ = worker.rows.send(rows.clone());
            }
        } else {
            self.worker = Some(Worker::start(rows.clone()));
        }
        self.rows = rows;
    }
}

impl Worker {
    fn available(&self) -> bool {
        self.ready.load(Ordering::Acquire) && self.online.load(Ordering::Acquire)
    }

    fn start(mut rows: Vec<Row>) -> Self {
        let (rows_tx, rows_rx) = mpsc::channel();
        let (commands_tx, commands_rx) = mpsc::channel();
        let ready = Arc::new(AtomicBool::new(false));
        let online = Arc::new(AtomicBool::new(false));
        let worker = Self {
            rows: rows_tx,
            commands: commands_rx,
            ready: ready.clone(),
            online: online.clone(),
        };
        std::thread::spawn(move || {
            // Always clear availability, including if the worker unwinds.
            struct Reset(Arc<AtomicBool>);
            impl Drop for Reset {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Release);
                }
            }
            let _reset = Reset(ready.clone());
            let icons = match icon::pixmaps() {
                Ok(icons) => icons,
                Err(error) => {
                    eprintln!("colima-gpui: tray icon unavailable: {error}");
                    return;
                }
            };
            let mut handle: Option<ksni::blocking::Handle<LinuxTray>> = None;
            loop {
                if handle.as_ref().is_some_and(|h| h.is_closed()) {
                    handle = None;
                    ready.store(false, Ordering::Release);
                }
                if handle.is_none() {
                    online.store(true, Ordering::Release);
                    match (LinuxTray {
                        rows: rows.clone(),
                        commands: commands_tx.clone(),
                        online: online.clone(),
                        icons: icons.clone(),
                    })
                    .spawn()
                    {
                        Ok(spawned) => {
                            handle = Some(spawned);
                            ready.store(true, Ordering::Release);
                        }
                        Err(_) => {
                            online.store(false, Ordering::Release);
                        }
                    }
                }
                match rows_rx.recv_timeout(Duration::from_secs(2)) {
                    Ok(latest) => {
                        rows = latest;
                        // Collapse queued polls before updating the D-Bus menu.
                        for latest in rows_rx.try_iter() {
                            rows = latest;
                        }
                        if let Some(handle) = &handle {
                            handle.update(|tray| tray.rows = rows.clone());
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            if let Some(handle) = handle {
                handle.shutdown().wait();
            }
        });
        worker
    }
}

struct LinuxTray {
    rows: Vec<Row>,
    commands: Sender<Command>,
    online: Arc<AtomicBool>,
    icons: Vec<ksni::Icon>,
}

impl ksni::Tray for LinuxTray {
    const MENU_ON_ACTIVATE: bool = true;
    fn id(&self) -> String {
        "colima-gpui".into()
    }
    fn title(&self) -> String {
        APP_NAME.into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        build_menu(&self.rows)
    }
    fn watcher_online(&self) {
        self.online.store(true, Ordering::Release);
    }
    fn watcher_offline(&self, _: ksni::OfflineReason) -> bool {
        self.online.store(false, Ordering::Release);
        true // Keep watching for the desktop's tray host to return.
    }
}

fn item(command: Command, label: &str, enabled: bool) -> ksni::MenuItem<LinuxTray> {
    StandardItem {
        label: label.replace('_', "__"),
        enabled,
        activate: Box::new(move |tray: &mut LinuxTray| {
            let _ = tray.commands.send(command.clone());
        }),
        ..Default::default()
    }
    .into()
}

fn build_menu(rows: &[Row]) -> Vec<ksni::MenuItem<LinuxTray>> {
    use ksni::{MenuItem, menu::SubMenu};
    let mut menu = Vec::new();
    if rows.is_empty() {
        menu.push(
            StandardItem {
                label: "No profiles".into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        );
    }
    for row in rows {
        let name = row.name.clone();
        let idle = row.state != State::Busy;
        let mut actions = if row.state == State::Running {
            vec![
                item(Command::Stop(name.clone()), "Stop", idle),
                item(Command::Restart(name.clone()), "Restart", idle),
            ]
        } else {
            vec![item(Command::Start(name.clone()), "Start", idle)]
        };
        actions.push(MenuItem::Separator);
        actions.push(item(
            Command::Show(Some(name)),
            &format!("Open in {APP_NAME}"),
            true,
        ));
        menu.push(
            SubMenu {
                label: row.title.replace('_', "__"),
                submenu: actions,
                ..Default::default()
            }
            .into(),
        );
    }
    menu.extend([
        MenuItem::Separator,
        item(Command::Show(None), &format!("Open {APP_NAME}"), true),
        item(Command::NewProfile, "New Profile…", true),
        item(Command::Settings, "Settings…", true),
        MenuItem::Separator,
        item(Command::Quit, &format!("Quit {APP_NAME}"), true),
    ]);
    menu
}

fn run(command: Command, cx: &mut App) {
    let store = ProfileStore::global(cx);
    let op = |op: Op, name: String, cx: &mut App| {
        store.update(cx, |store, cx| store.run(op, name, cx));
    };
    match command {
        Command::Start(name) => op(Op::Start, name, cx),
        Command::Stop(name) => op(Op::Stop, name, cx),
        Command::Restart(name) => op(Op::Restart, name, cx),
        Command::Show(name) => show_window(name, cx),
        Command::Settings => {
            show_window(None, cx);
            store.update(cx, |store, cx| store.select_settings(cx));
        }
        Command::NewProfile => {
            show_window(None, cx);
            store.update(cx, |store, cx| store.select_new_profile(cx));
        }
        Command::Quit => cx.quit(),
    }
}

fn show_window(profile: Option<String>, cx: &mut App) {
    if let Some(window) = cx.windows().into_iter().next() {
        window
            .update(cx, |_, window, _| window.activate_window())
            .ok();
    } else if let Err(error) = app::open_main_window(cx) {
        eprintln!("colima-gpui: could not open the window: {error}");
    }
    if let Some(name) = profile {
        ProfileStore::global(cx).update(cx, |store, cx| store.select(&name, cx));
    }
    cx.activate(true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ksni::MenuItem;

    #[test]
    fn menus_preserve_status_busy_protection_and_commands() {
        let rows = vec![
            Row {
                name: "t54_one".into(),
                title: "t54_one — Running".into(),
                state: State::Running,
            },
            Row {
                name: "t54-busy".into(),
                title: "t54-busy — Starting…".into(),
                state: State::Busy,
            },
            Row {
                name: "t54-stopped".into(),
                title: "t54-stopped — Stopped".into(),
                state: State::Stopped,
            },
        ];
        let (tx, rx) = mpsc::channel();
        let mut tray = LinuxTray {
            rows,
            commands: tx,
            online: Arc::new(AtomicBool::new(true)),
            icons: vec![],
        };
        let menu = build_menu(&tray.rows);
        let MenuItem::SubMenu(running) = &menu[0] else {
            panic!()
        };
        assert_eq!(running.label, "t54__one — Running");
        let MenuItem::Standard(stop) = &running.submenu[0] else {
            panic!()
        };
        let MenuItem::Standard(restart) = &running.submenu[1] else {
            panic!()
        };
        assert!(stop.enabled && restart.enabled);
        (stop.activate)(&mut tray);
        (restart.activate)(&mut tray);
        assert_eq!(
            rx.try_iter().collect::<Vec<_>>(),
            [
                Command::Stop("t54_one".into()),
                Command::Restart("t54_one".into())
            ]
        );
        let MenuItem::SubMenu(busy) = &menu[1] else {
            panic!()
        };
        let MenuItem::Standard(start) = &busy.submenu[0] else {
            panic!()
        };
        assert!(!start.enabled);
        let MenuItem::SubMenu(stopped) = &menu[2] else {
            panic!()
        };
        let MenuItem::Standard(start) = &stopped.submenu[0] else {
            panic!()
        };
        assert!(start.enabled);
        (start.activate)(&mut tray);
        assert_eq!(rx.recv().unwrap(), Command::Start("t54-stopped".into()));
        let mut commands = Vec::new();
        for entry in &menu[4..] {
            if let MenuItem::Standard(item) = entry {
                (item.activate)(&mut tray);
                commands.push(rx.recv().unwrap());
            }
        }
        assert_eq!(
            commands,
            [
                Command::Show(None),
                Command::NewProfile,
                Command::Settings,
                Command::Quit
            ]
        );
    }

    /// Run under dbus-run-session, optionally with a mock StatusNotifierWatcher/host.
    #[test]
    #[ignore = "requires an isolated D-Bus session; T54_EXPECT_HOST=1 expects a tray host"]
    fn live_status_notifier_registration() {
        let worker = Worker::start(vec![Row {
            name: "t54-smoke".into(),
            title: "t54-smoke — Stopped".into(),
            state: State::Stopped,
        }]);
        let expected = std::env::var("T54_EXPECT_HOST").as_deref() == Ok("1");
        // Allow startup and one retry, including on a busy build VM.
        for _ in 0..50 {
            if expected && worker.available() {
                break;
            }
            std::thread::sleep(POLL);
        }
        assert_eq!(worker.available(), expected);
        if expected {
            assert_eq!(
                worker
                    .commands
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap(),
                Command::Show(Some("t54-smoke".into()))
            );
            // The mock host exits after activating Open, exercising watcher loss.
            for _ in 0..50 {
                if !worker.available() {
                    break;
                }
                std::thread::sleep(POLL);
            }
            assert!(!worker.available());
        }
    }

    #[test]
    fn availability_requires_registration_and_a_live_host() {
        use ksni::Tray as _;
        let (tx, _) = mpsc::channel();
        let tray = LinuxTray {
            rows: vec![],
            commands: tx,
            online: Arc::new(AtomicBool::new(true)),
            icons: vec![],
        };
        let (rows, _) = mpsc::channel();
        let (_, commands) = mpsc::channel();
        let worker = Worker {
            rows,
            commands,
            ready: Arc::new(AtomicBool::new(false)),
            online: tray.online.clone(),
        };
        assert!(!worker.available());
        worker.ready.store(true, Ordering::Release);
        assert!(worker.available());
        assert!(tray.watcher_offline(ksni::OfflineReason::No));
        assert!(!worker.available());
        tray.watcher_online();
        assert!(worker.available());
    }
}
