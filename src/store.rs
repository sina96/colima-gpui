//! Shared profile state: polling, selection and profile actions.
//!
//! Views observe the [`ProfileStore`] entity and re-render when it notifies.
//! All CLI calls run on the background executor.

use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::sync::mpsc;
use std::time::Duration;

use gpui::{App, AppContext, Context, Entity, Global, SharedString, Subscription, Task};

use crate::colima::commands::{self, LogLine};
use crate::colima::model::{ColimaVersion, StartOptions};
use crate::colima::{CliError, Instance, Status};
use crate::settings;
use crate::views::log_panel::{LogFinished, LogPanel};

#[derive(Debug, Clone, PartialEq)]
pub enum LoadState {
    Loading,
    Ready,
    /// The `colima` binary could not be found.
    ColimaMissing,
    Failed(SharedString),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    None,
    Profile(String),
    NewProfile,
    EditProfile(String),
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Start,
    Stop,
    Restart,
    Delete,
    /// `colima delete --data`: also removes the profile's container data disk.
    DeleteWithData,
}

impl Op {
    pub fn progress_label(self) -> &'static str {
        match self {
            Op::Start => "Starting…",
            Op::Stop => "Stopping…",
            Op::Restart => "Restarting…",
            Op::Delete | Op::DeleteWithData => "Deleting…",
        }
    }

    pub fn is_delete(self) -> bool {
        matches!(self, Op::Delete | Op::DeleteWithData)
    }
}

pub struct ProfileStore {
    instances: Vec<Instance>,
    load: LoadState,
    selection: Selection,
    /// Where "Back" returns to from Settings.
    before_settings: Option<Selection>,
    /// `colima status` for running profiles, fetched for the selected one.
    statuses: HashMap<String, Status>,
    busy: HashMap<String, Op>,
    /// Last failed action per profile, shown inline until the next action.
    errors: HashMap<String, SharedString>,
    /// Live or finished `colima start` output, kept until dismissed or the next start.
    logs: HashMap<String, StartLog>,
    version: Option<ColimaVersion>,
    refreshing: bool,
    refresh_again: bool,
    fetching_status: HashSet<String>,
    /// The profiles marked "Start when Colima GPUI opens" were started (once per launch,
    /// after the first successful `colima list`).
    started_on_open: bool,
    _poll: Task<()>,
}

struct StartLog {
    panel: Entity<LogPanel>,
    _finished: Subscription,
}

/// Lets app-level actions (menu items) reach the store.
pub struct GlobalProfileStore(pub Entity<ProfileStore>);

impl Global for GlobalProfileStore {}

impl ProfileStore {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                let Ok(interval) = this.update(cx, |this, cx| {
                    this.refresh(cx);
                    poll_interval(cx)
                }) else {
                    break;
                };
                cx.background_executor().timer(interval).await;
            }
        });

        let version = cx
            .background_executor()
            .spawn(async { commands::version() });
        cx.spawn(async move |this, cx| {
            if let Ok(version) = version.await {
                this.update(cx, |this, cx| {
                    this.version = Some(version);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();

        Self {
            instances: Vec::new(),
            load: LoadState::Loading,
            selection: Selection::None,
            before_settings: None,
            statuses: HashMap::new(),
            busy: HashMap::new(),
            errors: HashMap::new(),
            logs: HashMap::new(),
            version: None,
            refreshing: false,
            refresh_again: false,
            fetching_status: HashSet::new(),
            started_on_open: false,
            _poll: poll,
        }
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalProfileStore>().0.clone()
    }

    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    pub fn load_state(&self) -> &LoadState {
        &self.load
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn selected_instance(&self) -> Option<&Instance> {
        match &self.selection {
            Selection::Profile(name) => self.instances.iter().find(|i| &i.name == name),
            _ => None,
        }
    }

    pub fn status(&self, profile: &str) -> Option<&Status> {
        self.statuses.get(profile)
    }

    pub fn busy(&self, profile: &str) -> Option<Op> {
        self.busy.get(profile).copied()
    }

    pub fn error(&self, profile: &str) -> Option<&SharedString> {
        self.errors.get(profile)
    }

    pub fn log(&self, profile: &str) -> Option<Entity<LogPanel>> {
        self.logs.get(profile).map(|log| log.panel.clone())
    }

    /// Hides a finished start log. A running start keeps its log.
    pub fn dismiss_log(&mut self, profile: &str, cx: &mut Context<Self>) {
        if self.busy(profile) != Some(Op::Start) {
            self.logs.remove(profile);
            cx.notify();
        }
    }

    pub fn version(&self) -> Option<&ColimaVersion> {
        self.version.as_ref()
    }

    pub fn select(&mut self, profile: &str, cx: &mut Context<Self>) {
        self.selection = Selection::Profile(profile.to_owned());
        self.fetch_status(profile, cx);
        cx.notify();
    }

    pub fn select_first(&mut self, cx: &mut Context<Self>) {
        self.selection = self
            .instances
            .first()
            .map_or(Selection::None, |i| Selection::Profile(i.name.clone()));
        cx.notify();
    }

    pub fn select_settings(&mut self, cx: &mut Context<Self>) {
        if self.selection != Selection::Settings {
            self.before_settings =
                Some(std::mem::replace(&mut self.selection, Selection::Settings));
            cx.notify();
        }
    }

    /// Leaves Settings for the selection it was opened from (or the first profile).
    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        if self.selection != Selection::Settings {
            return;
        }
        let exists = |name: &String| self.instances.iter().any(|i| &i.name == name);
        match self.before_settings.take() {
            Some(Selection::Profile(name)) | Some(Selection::EditProfile(name))
                if exists(&name) =>
            {
                self.select(&name, cx)
            }
            Some(Selection::NewProfile) => self.select_new_profile(cx),
            _ => self.select_first(cx),
        }
    }

    pub fn select_new_profile(&mut self, cx: &mut Context<Self>) {
        self.selection = Selection::NewProfile;
        cx.notify();
    }

    pub fn select_edit(&mut self, profile: &str, cx: &mut Context<Self>) {
        self.selection = Selection::EditProfile(profile.to_owned());
        cx.notify();
    }

    /// Runs `colima start` with its output streamed into a [`LogPanel`].
    ///
    /// `opts: None` starts with the saved config. `stop_first` stops a running
    /// profile so changed settings apply (colima ignores flags on a running VM).
    pub fn start(
        &mut self,
        profile: String,
        opts: Option<StartOptions>,
        stop_first: bool,
        cx: &mut Context<Self>,
    ) {
        if self.busy.contains_key(&profile) {
            return;
        }
        self.busy.insert(profile.clone(), Op::Start);
        self.errors.remove(&profile);
        self.statuses.remove(&profile);

        let (tx, rx) = mpsc::channel();
        let name = profile.clone();
        std::thread::spawn(move || {
            if stop_first {
                let _ = tx.send(LogLine::Stdout(
                    "Stopping the profile to apply changes…".into(),
                ));
                if let Err(e) = commands::stop(&name) {
                    let _ = tx.send(LogLine::Stderr(e.to_string()));
                    let _ = tx.send(LogLine::Exit(-1));
                    return;
                }
            }
            for line in commands::start_streaming(&name, opts.as_ref()) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        let save_name = format!("colima-{profile}");
        let panel = cx.new(|cx| {
            LogPanel::new(rx, cx)
                .running_label("Starting…")
                .save_name(save_name)
        });
        let name = profile.clone();
        let finished = cx.subscribe(&panel, move |this, _, event: &LogFinished, cx| {
            this.busy.remove(&name);
            if event.exit_code != Some(0) {
                let message = match event.exit_code {
                    Some(code) => format!("colima start failed (exit {code}). See the log below."),
                    None => "colima start stopped reporting progress. See the log below.".into(),
                };
                this.errors.insert(name.clone(), message.into());
            }
            this.refresh(cx);
            cx.notify();
        });
        self.logs.insert(
            profile,
            StartLog {
                panel,
                _finished: finished,
            },
        );
        cx.notify();
        self.refresh(cx);
    }

    /// Reloads `colima list`. Calls made while a reload is in flight queue one more.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.refreshing {
            self.refresh_again = true;
            return;
        }
        self.refreshing = true;
        let task = cx.background_executor().spawn(async { commands::list() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.refreshing = false;
                this.apply_list(result, cx);
                if std::mem::take(&mut this.refresh_again) {
                    this.refresh(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn run(&mut self, op: Op, profile: String, cx: &mut Context<Self>) {
        if op == Op::Start {
            return self.start(profile, None, false, cx);
        }
        if self.busy.contains_key(&profile) {
            return;
        }
        self.busy.insert(profile.clone(), op);
        self.errors.remove(&profile);
        cx.notify();

        let name = profile.clone();
        let task = cx.background_executor().spawn(async move {
            match op {
                Op::Start => unreachable!("Op::Start streams through ProfileStore::start"),
                Op::Stop => commands::stop(&name),
                Op::Restart => commands::restart(&name),
                Op::Delete => commands::delete(&name, false),
                Op::DeleteWithData => commands::delete(&name, true),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.busy.remove(&profile);
                this.statuses.remove(&profile);
                if op.is_delete() && result.is_ok() {
                    this.logs.remove(&profile);
                    settings::update(cx, |s| s.set_start_on_open(&profile, false));
                }
                if let Err(e) = result {
                    this.errors.insert(profile, e.to_string().into());
                }
                this.refresh(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn apply_list(&mut self, result: Result<Vec<Instance>, CliError>, cx: &mut Context<Self>) {
        match result {
            Ok(instances) => {
                self.instances = instances;
                self.load = LoadState::Ready;
                if !std::mem::replace(&mut self.started_on_open, true) {
                    let wanted = settings::get(cx).start_on_open.clone();
                    for name in profiles_to_start(&self.instances, &wanted) {
                        self.start(name, None, false, cx);
                    }
                }
            }
            Err(CliError::Spawn { source, .. }) if source.kind() == ErrorKind::NotFound => {
                self.instances.clear();
                self.load = LoadState::ColimaMissing;
            }
            // Keep showing the last good list; surface the error only if there is none.
            Err(e) if self.instances.is_empty() => {
                self.load = LoadState::Failed(e.to_string().into())
            }
            Err(_) => {}
        }

        let running: HashSet<&str> = self
            .instances
            .iter()
            .filter(|i| i.is_running())
            .map(|i| i.name.as_str())
            .collect();
        self.statuses
            .retain(|name, _| running.contains(name.as_str()));

        let exists = |name: &String| self.instances.iter().any(|i| &i.name == name);
        let selection_valid = match &self.selection {
            // A profile being created has a log before `colima list` shows it.
            Selection::Profile(name) => exists(name) || self.logs.contains_key(name),
            Selection::EditProfile(name) => exists(name),
            Selection::None => false,
            Selection::NewProfile | Selection::Settings => true,
        };
        if !selection_valid {
            self.selection = self
                .instances
                .first()
                .map_or(Selection::None, |i| Selection::Profile(i.name.clone()));
        }
        if let Selection::Profile(name) = self.selection.clone() {
            self.fetch_status(&name, cx);
        }
        cx.notify();
    }

    fn fetch_status(&mut self, profile: &str, cx: &mut Context<Self>) {
        let running = self
            .instances
            .iter()
            .any(|i| i.name == profile && i.is_running());
        if !running
            || self.statuses.contains_key(profile)
            || !self.fetching_status.insert(profile.to_owned())
        {
            return;
        }
        let name = profile.to_owned();
        let task = cx.background_executor().spawn({
            let name = name.clone();
            async move { commands::status(&name) }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.fetching_status.remove(&name);
                if let Ok(status) = result {
                    this.statuses.insert(name, status);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

/// The user's poll interval, or the default before settings are loaded.
/// The profiles in `wanted` that exist and are stopped, in `wanted`'s order. A profile in
/// another state (e.g. Broken) is left alone.
fn profiles_to_start(instances: &[Instance], wanted: &[String]) -> Vec<String> {
    wanted
        .iter()
        .filter(|name| {
            instances
                .iter()
                .any(|i| &i.name == *name && i.status == "Stopped")
        })
        .cloned()
        .collect()
}

pub fn poll_interval(cx: &App) -> Duration {
    cx.try_global::<settings::Settings>().map_or_else(
        || settings::Settings::default().poll_interval(),
        |s| s.poll_interval(),
    )
}

/// Creates the store and registers it as a global for app-level actions.
pub fn init(cx: &mut App) -> Entity<ProfileStore> {
    let store = cx.new(ProfileStore::new);
    cx.set_global(GlobalProfileStore(store.clone()));
    store
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(name: &str, status: &str) -> Instance {
        serde_json::from_str(&format!(r#"{{"name":"{name}","status":"{status}"}}"#)).unwrap()
    }

    #[test]
    fn only_wanted_profiles_that_exist_and_are_stopped_start() {
        let instances = [
            instance("default", "Stopped"),
            instance("work", "Running"),
            instance("gpui-test", "Stopped"),
            instance("other", "Stopped"),
            instance("broken", "Broken"),
        ];
        let wanted = ["gpui-test", "work", "deleted", "broken", "default"].map(String::from);
        assert_eq!(
            profiles_to_start(&instances, &wanted),
            ["gpui-test", "default"]
        );
        assert!(profiles_to_start(&instances, &[]).is_empty());
    }
}
