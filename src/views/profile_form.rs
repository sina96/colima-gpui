//! Create/edit profile form producing [`StartOptions`], laid out like Zed's settings:
//! sections of title + description rows, with rarely used options behind "More".
//!
//! Create mode exposes every option. Edit mode only offers what colima can change
//! on an existing VM (CPUs, memory, growing the disk, mounts, Kubernetes, network address);
//! runtime, VM type, architecture and mount type are fixed after creation.
//! DNS servers are fixed too (checked on colima 0.10.3: a restart doesn't apply them).
//! Root disk and CPU type stay read-only in Edit until a restart is checked for them.

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::colima::Instance;
use crate::colima::model::{Mount, StartOptions};
use crate::theme::{ActiveTheme, Theme};
use crate::ui::button::ButtonSize;
use crate::ui::select::{Select, SelectChanged};
use crate::ui::stepper::{Stepper, StepperChanged};
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{Button, ButtonStyle, IconName, InfoTip, Switch};

const RUNTIMES: &[&str] = &["docker", "containerd", "incus"];
#[cfg(not(target_os = "linux"))]
const VM_TYPES: &[&str] = &["vz", "qemu", "krunkit"];
#[cfg(target_os = "linux")]
const VM_TYPES: &[&str] = &["qemu"];
const ARCHES: &[&str] = &["aarch64", "x86_64"];
#[cfg(not(target_os = "linux"))]
const MOUNT_TYPES: &[&str] = &["virtiofs", "sshfs", "9p"];
#[cfg(target_os = "linux")]
const MOUNT_TYPES: &[&str] = &["sshfs", "9p"];
/// Blocks saving an unchanged profile; the disabled button says enough, so it isn't shown.
const NO_CHANGES: &str = "No changes yet.";
const MAX_MEMORY_GIB: i32 = 64;
const MAX_DISK_GIB: i32 = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormMode {
    Create {
        existing: Vec<String>,
    },
    Edit {
        profile: String,
        running: bool,
        min_disk_gib: u32,
    },
}

// Kept inline to preserve the form event API; there is only one payload variant.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileFormEvent {
    Cancelled,
    Submitted {
        profile: String,
        options: StartOptions,
        stop_first: bool,
    },
}

/// A form section with less common options behind "More".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Resources,
    Vm,
    Mounts,
    Network,
    Kubernetes,
}

/// What the VM sees of this computer: Colima's default (home), a list, or nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MountChoice {
    Home,
    Custom,
    None,
}

struct MountRow {
    id: usize,
    path: Entity<TextInput>,
    writable: bool,
    mount_point: String,
    _subscription: Subscription,
}

struct OptionRow {
    id: usize,
    input: Entity<TextInput>,
    _subscription: Subscription,
}

pub struct ProfileForm {
    mode: FormMode,
    /// Settings the form started from; edit mode compares against it.
    initial: StartOptions,
    name: Entity<TextInput>,
    name_touched: bool,
    cpus: Entity<Stepper>,
    memory: Entity<Stepper>,
    disk: Entity<Stepper>,
    runtime: Entity<Select<&'static str>>,
    vm_type: Entity<Select<&'static str>>,
    arch: Entity<Select<&'static str>>,
    mount_type: Entity<Select<&'static str>>,
    mounts: Vec<MountRow>,
    mount_choice: MountChoice,
    next_mount_id: usize,
    kubernetes: bool,
    kubernetes_version: Entity<TextInput>,
    rosetta: bool,
    /// Sections whose less common options are shown ("More").
    more: Vec<Section>,
    network_address: bool,
    dns: Entity<TextInput>,
    network_mode: Entity<Select<&'static str>>,
    network_interface: Entity<TextInput>,
    mount_inotify: bool,
    hostname: Entity<TextInput>,
    env: Vec<OptionRow>,
    k3s_args: Vec<OptionRow>,
    next_option_id: usize,
    ssh_agent: bool,
    root_disk: Entity<Stepper>,
    cpu_type: Entity<TextInput>,
    nested_virtualization: bool,
    nested_supported: bool,
    port_forwarder: Entity<Select<&'static str>>,
    editor_error: Option<String>,
    activate: bool,
    load_warning: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ProfileFormEvent> for ProfileForm {}

impl ProfileForm {
    pub fn create(existing: Vec<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let initial = StartOptions::default();
        let this = Self::build(FormMode::Create { existing }, initial, None, cx);
        window.focus(&this.name.focus_handle(cx), cx);
        this
    }

    pub fn edit(instance: &Instance, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // A small local file read, not a CLI call.
        let (config, warning) = match StartOptions::read_config(&instance.name) {
            Ok(config) => (config, None),
            Err(e) => (
                StartOptions::default(),
                Some(
                    format!(
                        "Couldn't read colima.yaml ({e}). Open the file to fix it, then reopen Edit."
                    )
                    .into(),
                ),
            ),
        };
        // `colima list` is the source of truth for what the VM has now.
        let initial = StartOptions {
            cpus: instance.cpus,
            memory_gib: gib(instance.memory),
            disk_gib: gib(instance.disk),
            runtime: instance.runtime.clone(),
            arch: instance.arch.clone(),
            ..config
        };
        let mode = FormMode::Edit {
            profile: instance.name.clone(),
            running: instance.is_running(),
            min_disk_gib: initial.disk_gib,
        };
        let this = Self::build(mode, initial, warning, cx);
        window.focus(&this.cpus.focus_handle(cx), cx);
        this
    }

    fn build(
        mode: FormMode,
        mut initial: StartOptions,
        load_warning: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        let min_disk = match &mode {
            FormMode::Edit { min_disk_gib, .. } => *min_disk_gib as i32,
            FormMode::Create { .. } => 10,
        };
        let max_cpus = std::thread::available_parallelism().map_or(8, |n| n.get() as i32);

        let name = cx.new(|cx| TextInput::new(cx).placeholder("e.g. dev"));
        let cpus = cx.new(|cx| {
            Stepper::new(
                initial.cpus as i32,
                1,
                max_cpus.max(initial.cpus as i32),
                1,
                cx,
            )
        });
        let memory = cx.new(|cx| {
            Stepper::new(
                initial.memory_gib as i32,
                1,
                MAX_MEMORY_GIB.max(initial.memory_gib as i32),
                1,
                cx,
            )
            .unit("GiB")
        });
        let disk = cx.new(|cx| {
            Stepper::new(
                initial.disk_gib as i32,
                min_disk,
                MAX_DISK_GIB.max(min_disk),
                10,
                cx,
            )
            .unit("GiB")
        });
        let runtime = select(RUNTIMES, &initial.runtime, cx);
        let vm_type = select(VM_TYPES, &initial.vm_type, cx);
        let arch = select(ARCHES, &initial.arch, cx);
        let mount_type = select(MOUNT_TYPES, &initial.mount_type, cx);
        let kubernetes_version = text_input(&initial.kubernetes_version, "Colima default", cx);
        let dns = text_input(&initial.dns.join(", "), "e.g. 1.1.1.1", cx);
        let network_mode = select(&["shared", "bridged"], &initial.network_mode, cx);
        let network_interface = text_input(&initial.network_interface, "e.g. en0", cx);

        let hostname = text_input(&initial.hostname, "Colima default", cx);
        let cpu_type = text_input(&initial.cpu_type, "QEMU default", cx);
        let root_disk = cx.new(|cx| {
            Stepper::new(
                initial.root_disk_gib as i32,
                1,
                MAX_DISK_GIB.max(initial.root_disk_gib as i32),
                1,
                cx,
            )
            .unit("GiB")
        });
        // Preserve CLI-created 'none' even though the form offers ssh and grpc.
        let port_forwarder = select(
            if initial.port_forwarder == "none" {
                &["ssh", "grpc", "none"]
            } else {
                &["ssh", "grpc"]
            },
            &initial.port_forwarder,
            cx,
        );
        let env: Vec<_> = initial
            .env
            .iter()
            .enumerate()
            .map(|(id, (key, value))| option_row(id, &format!("{key}={value}"), "KEY=value", cx))
            .collect();
        let k3s_args: Vec<_> = initial
            .k3s_args
            .iter()
            .enumerate()
            .map(|(id, value)| option_row(env.len() + id, value, "--disable=traefik", cx))
            .collect();
        let next_option_id = env.len() + k3s_args.len();

        let mounts: Vec<_> = initial
            .mounts
            .as_deref()
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(id, mount)| mount_row(id, mount, cx))
            .collect();
        let next_mount_id = mounts.len();
        let mut subscriptions =
            vec![
                cx.subscribe(&name, |this, _, event: &TextInputEvent, cx| match event {
                    TextInputEvent::Changed => {
                        this.name_touched = true;
                        cx.notify();
                    }
                    TextInputEvent::Submitted => this.submit(cx),
                }),
            ];
        for stepper in [&cpus, &memory, &disk, &root_disk] {
            subscriptions.push(cx.subscribe(stepper, |_, _, _: &StepperChanged, cx| cx.notify()));
        }
        for select in [
            &runtime,
            &vm_type,
            &arch,
            &mount_type,
            &network_mode,
            &port_forwarder,
        ] {
            subscriptions.push(
                cx.subscribe(select, |_, _, _: &SelectChanged<&'static str>, cx| {
                    cx.notify()
                }),
            );
        }
        for input in [
            &kubernetes_version,
            &dns,
            &network_interface,
            &hostname,
            &cpu_type,
        ] {
            subscriptions.push(subscribe_input(input, cx));
        }
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            let task = cx
                .background_executor()
                .spawn(async { nested_virtualization_supported() });
            cx.spawn(async move |this, cx| {
                let supported = task.await;
                let _ = this.update(cx, |this, cx| {
                    this.nested_supported = supported;
                    this.initial.nested_virtualization =
                        supported.then_some(this.nested_virtualization);
                    cx.notify();
                });
            })
            .detach();
        }
        let nested_virtualization = initial.nested_virtualization.unwrap_or(false);
        initial.nested_virtualization = None;
        Self {
            mount_inotify: initial.mount_inotify,
            hostname,
            env,
            k3s_args,
            next_option_id,
            ssh_agent: initial.ssh_agent,
            root_disk,
            cpu_type,
            nested_virtualization,
            nested_supported: false,
            port_forwarder,
            editor_error: None,
            mode,
            dns,
            network_mode,
            network_interface,
            mount_choice: match &initial.mounts {
                None => MountChoice::None,
                Some(mounts) if mounts.is_empty() => MountChoice::Home,
                Some(_) => MountChoice::Custom,
            },
            mounts,
            next_mount_id,
            kubernetes: initial.kubernetes,
            kubernetes_version,
            rosetta: initial.rosetta,
            more: Vec::new(),
            network_address: initial.network_address,
            activate: initial.activate,
            initial,
            name,
            name_touched: false,
            cpus,
            memory,
            disk,
            runtime,
            vm_type,
            arch,
            mount_type,
            load_warning,
            _subscriptions: subscriptions,
        }
    }

    fn creating(&self) -> bool {
        matches!(self.mode, FormMode::Create { .. })
    }

    fn options(&self, cx: &App) -> StartOptions {
        let creating = self.creating();
        let value = |select: &Entity<Select<&'static str>>| {
            select
                .read(cx)
                .value()
                .map_or(String::new(), |v| (*v).to_owned())
        };
        StartOptions {
            cpus: self.cpus.read(cx).value() as u32,
            memory_gib: self.memory.read(cx).value() as u32,
            disk_gib: self.disk.read(cx).value() as u32,
            runtime: value(&self.runtime),
            vm_type: value(&self.vm_type),
            arch: value(&self.arch),
            kubernetes: self.kubernetes,
            kubernetes_version: self.kubernetes_version.read(cx).text().trim().to_owned(),
            rosetta: self.rosetta && value(&self.vm_type) == "vz",
            mount_type: value(&self.mount_type),
            mounts: match self.mount_choice {
                MountChoice::None => None,
                MountChoice::Home => Some(Vec::new()),
                MountChoice::Custom => Some(
                    self.mounts
                        .iter()
                        .map(|row| Mount {
                            location: row.path.read(cx).text().trim().to_owned(),
                            writable: row.writable,
                            mount_point: row.mount_point.clone(),
                        })
                        .collect(),
                ),
            },
            network_address: self.network_address,
            // Colima 0.10.3 writes the VM's dnsmasq servers only when it creates the VM.
            dns: if creating {
                dns_servers(self.dns.read(cx).text())
            } else {
                self.initial.dns.clone()
            },
            network_mode: if creating {
                value(&self.network_mode)
            } else {
                self.initial.network_mode.clone()
            },
            network_interface: self.network_interface.read(cx).text().trim().to_owned(),
            mount_inotify: self.mount_inotify,
            // Applied on restart (checked on colima 0.10.3).
            hostname: self.hostname.read(cx).text().trim().to_owned(),
            env: self
                .env
                .iter()
                .filter_map(|row| row.input.read(cx).text().split_once('='))
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
            k3s_args: self
                .k3s_args
                .iter()
                .map(|row| row.input.read(cx).text().to_owned())
                .collect(),
            ssh_agent: self.ssh_agent,
            // The root disk is separate from the resizable runtime data disk.
            root_disk_gib: if creating {
                self.root_disk.read(cx).value() as u32
            } else {
                self.initial.root_disk_gib
            },
            cpu_type: if creating {
                self.cpu_type.read(cx).text().trim().to_owned()
            } else {
                self.initial.cpu_type.clone()
            },
            nested_virtualization: self.nested_supported.then_some(self.nested_virtualization),
            port_forwarder: value(&self.port_forwarder),
            activate: self.activate,
        }
    }

    fn render_option_list(&self, environment: bool, cx: &mut Context<Self>) -> AnyElement {
        let rows = if environment {
            &self.env
        } else {
            &self.k3s_args
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .py_2()
            .children(rows.iter().map(|row| {
                let id = row.id;
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().flex_1().min_w_0().child(row.input.clone()))
                    .child(
                        Button::new(("remove-option", id), "Remove").on_click(cx.listener(
                            move |this, _, _, cx| {
                                let rows = if environment {
                                    &mut this.env
                                } else {
                                    &mut this.k3s_args
                                };
                                rows.retain(|row| row.id != id);
                                cx.notify();
                            },
                        )),
                    )
            }))
            .child(div().flex().child(
                Button::new(if environment { "add-env" } else { "add-k3s" }, "Add").on_click(
                    cx.listener(move |this, _, window, cx| {
                        let row = option_row(
                            this.next_option_id,
                            "",
                            if environment {
                                "KEY=value"
                            } else {
                                "--disable=traefik"
                            },
                            cx,
                        );
                        this.next_option_id += 1;
                        window.focus(&row.input.focus_handle(cx), cx);
                        if environment {
                            this.env.push(row);
                        } else {
                            this.k3s_args.push(row);
                        }
                        cx.notify();
                    }),
                ),
            ))
            .into_any_element()
    }

    fn open_config(&mut self, cx: &mut Context<Self>) {
        let FormMode::Edit { profile, .. } = &self.mode else {
            return;
        };
        if !crate::docker::system::valid_profile(profile) {
            return;
        }
        let Some(home) = crate::colima::paths::colima_home() else {
            self.editor_error = Some("HOME is not set.".into());
            cx.notify();
            return;
        };
        let path = home.join(profile).join("colima.yaml");
        self.editor_error = None;
        let task = cx.background_executor().spawn(async move {
            // Opening the editor is deliberately independent of the CLI runner.
            let mut command = std::process::Command::new(if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            });
            if cfg!(target_os = "macos") {
                command.arg("-t");
            }
            let status = command
                .arg(path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map_err(|error| format!("Couldn't open colima.yaml: {error}"))?;
            if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "Couldn't open colima.yaml: editor launcher exited with {status}"
                ))
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.editor_error = result.err();
                cx.notify();
            });
        })
        .detach();
    }

    fn profile_name(&self, cx: &App) -> String {
        match &self.mode {
            FormMode::Create { .. } => self.name.read(cx).text().trim().to_owned(),
            FormMode::Edit { profile, .. } => profile.clone(),
        }
    }

    /// The first problem blocking submit, if any.
    fn problem(&self, cx: &App) -> Option<String> {
        // Defaults from an unreadable/unsupported config must never overwrite it.
        if self.load_warning.is_some() {
            return Some("Fix colima.yaml and reopen Edit before saving.".into());
        }
        let options = self.options(cx);
        let mut keys = std::collections::BTreeSet::new();
        for row in &self.env {
            let Some((key, _)) = row.input.read(cx).text().split_once('=') else {
                return Some("Environment variables need KEY=value, one per row.".into());
            };
            if !keys.insert(key) {
                return Some("Environment variable names must be unique.".into());
            }
        }
        if self.mount_choice == MountChoice::Custom && self.mounts.is_empty() {
            return Some("Add a folder to share, or choose Home folder.".into());
        }
        if options.env.is_empty() && !self.initial.env.is_empty() {
            return Some(
                "To remove every environment variable, clear env in colima.yaml and reopen Edit."
                    .into(),
            );
        }
        match &self.mode {
            FormMode::Create { existing } => validate_name(&self.profile_name(cx), existing)
                .err()
                .or_else(|| validate_options(&options, host_arch()).err()),
            FormMode::Edit { .. } => {
                // `activate` only affects this start, so it alone isn't a change.
                let changed = StartOptions {
                    activate: self.initial.activate,
                    ..options.clone()
                };
                if changed == self.initial {
                    Some(NO_CHANGES.into())
                } else {
                    validate_options(&options, host_arch()).err()
                }
            }
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.problem(cx).is_some() {
            // Reveal the problem message even if the name was never typed in.
            self.name_touched = true;
            cx.notify();
            return;
        }
        let stop_first = matches!(self.mode, FormMode::Edit { running: true, .. });
        cx.emit(ProfileFormEvent::Submitted {
            profile: self.profile_name(cx),
            options: self.options(cx),
            stop_first,
        });
    }

    fn more_open(&self, section: Section) -> bool {
        self.more.contains(&section)
    }

    /// A small uppercase section heading.
    fn heading(&self, title: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        div()
            .pt_6()
            .pb_1()
            .border_b_1()
            .border_color(theme.border_variant)
            .text_xs()
            .text_color(theme.text_muted)
            .child(title.to_uppercase())
            .into_any_element()
    }

    /// "More options" under a section's last row, showing its rarely used options.
    fn more_toggle(&self, section: Section, cx: &mut Context<Self>) -> AnyElement {
        let open = self.more_open(section);
        div()
            .flex()
            .pt_1()
            .child(
                Button::new(
                    ("more", section as usize),
                    if open {
                        "Fewer options"
                    } else {
                        "More options"
                    },
                )
                .icon(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(index) = this.more.iter().position(|s| *s == section) {
                        this.more.remove(index);
                    } else {
                        this.more.push(section);
                    }
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    fn add_mount(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let row = mount_row(
            self.next_mount_id,
            &Mount {
                location: String::new(),
                writable: true,
                mount_point: String::new(),
            },
            cx,
        );
        self.next_mount_id += 1;
        window.focus(&row.path.focus_handle(cx), cx);
        self.mounts.push(row);
    }

    fn render_mount_choice(&self, cx: &mut Context<Self>) -> AnyElement {
        let choices = [
            ("Home folder", MountChoice::Home),
            ("Custom", MountChoice::Custom),
            ("None", MountChoice::None),
        ];
        div()
            .flex()
            .gap_1()
            .children(choices.into_iter().map(|(label, choice)| {
                Button::new(label, label)
                    .style(if self.mount_choice == choice {
                        ButtonStyle::Selected
                    } else {
                        ButtonStyle::Ghost
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.mount_choice = choice;
                        if choice == MountChoice::Custom && this.mounts.is_empty() {
                            this.add_mount(window, cx);
                        }
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }

    /// The custom folder list: path, writable switch and remove, one row per folder.
    fn render_mounts(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .py_2()
            .border_b_1()
            .border_color(theme.border_variant)
            .children(self.mounts.iter().map(|row| {
                let id = row.id;
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(row.path.clone()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_muted)
                            .child("Writable"),
                    )
                    .child(Switch::new(("mount-writable", id), row.writable).on_toggle(
                        cx.listener(move |this, _, _, cx| {
                            if let Some(row) = this.mounts.iter_mut().find(|row| row.id == id) {
                                row.writable = !row.writable;
                            }
                            cx.notify();
                        }),
                    ))
                    .when(!row.mount_point.is_empty(), |el| {
                        el.child(InfoTip::new(
                            ("mount-point", id),
                            format!("Guest path: {}", row.mount_point),
                        ))
                    })
                    .child(
                        Button::new(("mount-remove", id), "Remove").on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.mounts.retain(|row| row.id != id);
                                cx.notify();
                            },
                        )),
                    )
            }))
            .child(
                div()
                    .flex()
                    .child(Button::new("mount-add", "Add folder").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.add_mount(window, cx);
                            cx.notify();
                        },
                    ))),
            )
            .into_any_element()
    }

    /// Every section and row, top to bottom. Rows a profile can't change show their value.
    fn render_rows(&self, options: &StartOptions, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = cx.theme().clone();
        let t = &theme;
        let creating = self.creating();
        let fixed_or = |control: AnyElement, value: &str| {
            if creating {
                field(control)
            } else {
                fixed(t, value)
            }
        };
        let mut rows = Vec::new();

        rows.push(self.heading("Profile", cx));
        if creating {
            rows.push(item(
                t,
                "Name",
                note(t, "Lowercase letters, digits and dashes."),
                field(self.name.clone().into_any_element()),
            ));
        }
        rows.push(item(
            t,
            "Use as Docker context",
            note(t, "Switch Docker to this profile when it starts."),
            self.render_toggle(
                "activate",
                self.activate,
                |this| this.activate = !this.activate,
                cx,
            ),
        ));

        rows.push(self.heading("Resources", cx));
        rows.push(item(
            t,
            "CPUs",
            note(t, "Virtual CPUs for the VM."),
            field(self.cpus.clone().into_any_element()),
        ));
        rows.push(item(
            t,
            "Memory",
            // Checked on colima 0.10.3: with 1 GiB, k3s restarts every few seconds.
            if options.kubernetes && options.memory_gib < 2 {
                warning(
                    t,
                    "Kubernetes needs at least 2 GiB. With less, k3s keeps restarting.",
                )
            } else {
                note(t, "Memory for the VM.")
            },
            field(self.memory.clone().into_any_element()),
        ));
        rows.push(item(
            t,
            "Disk",
            note(
                t,
                "Space for containers, images and volumes. It can grow later, not shrink.",
            ),
            field(self.disk.clone().into_any_element()),
        ));
        if self.more_open(Section::Resources) {
            rows.push(item(
                t,
                "Root disk",
                note(t, "The VM's system disk. Set when the profile is created."),
                fixed_or(
                    self.root_disk.clone().into_any_element(),
                    &format!("{} GiB", options.root_disk_gib),
                ),
            ));
        }

        rows.push(self.more_toggle(Section::Resources, cx));
        rows.push(self.heading("VM", cx));
        rows.push(item(
            t,
            "Runtime",
            note(t, "Fixed after creation."),
            fixed_or(self.runtime.clone().into_any_element(), &options.runtime),
        ));
        rows.push(item(
            t,
            "VM type",
            note(
                t,
                if cfg!(target_os = "linux") {
                    "Fixed after creation."
                } else {
                    "vz is Apple's own and fastest. qemu can emulate other architectures. Fixed after creation."
                },
            ),
            fixed_or(self.vm_type.clone().into_any_element(), &options.vm_type),
        ));
        rows.push(item(
            t,
            "Architecture",
            note(
                t,
                "Another architecture than this computer's is emulated. Fixed after creation.",
            ),
            fixed_or(self.arch.clone().into_any_element(), &options.arch),
        ));
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) && options.vm_type == "vz" {
            rows.push(item(
                t,
                "Rosetta",
                note(t, "Run x86_64 containers quickly on Apple silicon."),
                self.render_toggle(
                    "rosetta",
                    self.rosetta,
                    |this| this.rosetta = !this.rosetta,
                    cx,
                ),
            ));
        }
        if self.more_open(Section::Vm) {
            rows.push(item(
                t,
                "SSH agent",
                note(t, "Forward this computer's SSH agent into the VM."),
                self.render_toggle(
                    "ssh-agent",
                    self.ssh_agent,
                    |this| this.ssh_agent = !this.ssh_agent,
                    cx,
                ),
            ));
            if options.vm_type == "qemu" {
                let cpu_type = if options.cpu_type.is_empty() {
                    "QEMU default"
                } else {
                    &options.cpu_type
                };
                rows.push(item(
                    t,
                    "CPU type",
                    note(
                        t,
                        "A QEMU CPU model, such as host or max. Set when the profile is created.",
                    ),
                    fixed_or(self.cpu_type.clone().into_any_element(), cpu_type),
                ));
            }
            if options.vm_type == "vz" && self.nested_supported {
                rows.push(item(
                    t,
                    "Nested virtualization",
                    note(t, "Run VMs inside the VM."),
                    self.render_toggle(
                        "nested-virtualization",
                        self.nested_virtualization,
                        |this| this.nested_virtualization = !this.nested_virtualization,
                        cx,
                    ),
                ));
            }
        }

        rows.push(self.more_toggle(Section::Vm, cx));
        rows.push(self.heading("Mounts", cx));
        rows.push(item(
            t,
            "Mount type",
            note(
                t,
                "How folders are shared with the VM. Fixed after creation.",
            ),
            fixed_or(
                self.mount_type.clone().into_any_element(),
                &options.mount_type,
            ),
        ));
        rows.push(item(
            t,
            "Shared folders",
            note(
                t,
                match self.mount_choice {
                    MountChoice::Home => "Your home folder, writable. Colima's default.",
                    MountChoice::Custom => "Only the folders below.",
                    MountChoice::None => "Nothing from this computer.",
                },
            ),
            self.render_mount_choice(cx),
        ));
        if self.mount_choice == MountChoice::Custom {
            rows.push(self.render_mounts(cx));
        }
        if self.more_open(Section::Mounts) {
            rows.push(item(
                t,
                "Inotify events",
                note(
                    t,
                    "Pass file changes to containers, for file watchers. Experimental.",
                ),
                self.render_toggle(
                    "mount-inotify",
                    self.mount_inotify,
                    |this| this.mount_inotify = !this.mount_inotify,
                    cx,
                ),
            ));
        }

        rows.push(self.more_toggle(Section::Mounts, cx));
        rows.push(self.heading("Network", cx));
        rows.push(item(
            t,
            "Network address",
            note(
                t,
                "Give the VM its own IP address, reachable from this computer.",
            ),
            self.render_toggle(
                "network-address",
                self.network_address,
                |this| this.network_address = !this.network_address,
                cx,
            ),
        ));
        // Colima registers mode and interface only on macOS, and they need an address.
        if cfg!(target_os = "macos") && self.network_address {
            rows.push(item(
                t,
                "Mode",
                note(
                    t,
                    "Shared, or bridged to a host interface. Fixed after creation.",
                ),
                fixed_or(
                    self.network_mode.clone().into_any_element(),
                    &options.network_mode,
                ),
            ));
            if options.network_mode == "bridged" {
                rows.push(item(
                    t,
                    "Host interface",
                    note(t, "The interface to bridge, such as en0."),
                    field(self.network_interface.clone().into_any_element()),
                ));
            }
        }
        if self.more_open(Section::Network) {
            rows.push(item(
                t,
                "Hostname",
                note(t, "Empty uses Colima's default."),
                field(self.hostname.clone().into_any_element()),
            ));
            rows.push(item(
                t,
                "Port forwarder",
                note(t, "How container ports reach this computer."),
                field(self.port_forwarder.clone().into_any_element()),
            ));
            let dns = if options.dns.is_empty() {
                "Colima default".to_owned()
            } else {
                options.dns.join(", ")
            };
            rows.push(item(
                t,
                "DNS servers",
                note(
                    t,
                    "IP addresses, separated by commas. Fixed after creation.",
                ),
                fixed_or(self.dns.clone().into_any_element(), &dns),
            ));
        }

        rows.push(self.more_toggle(Section::Network, cx));
        rows.push(self.heading("Kubernetes", cx));
        rows.push(item(
            t,
            "Kubernetes",
            note(
                t,
                "Run a k3s cluster in the VM and make it kubectl's current context.",
            ),
            self.render_toggle(
                "kubernetes",
                self.kubernetes,
                |this| this.kubernetes = !this.kubernetes,
                cx,
            ),
        ));
        if self.kubernetes {
            rows.push(item(
                t,
                "Version",
                note(
                    t,
                    "A k3s version, such as v1.35.0+k3s1. Empty uses Colima's default.",
                ),
                field(self.kubernetes_version.clone().into_any_element()),
            ));
            if self.more_open(Section::Kubernetes) {
                rows.push(stacked_item(
                    t,
                    "k3s arguments",
                    note(t, "Extra arguments for the k3s server, one per row."),
                    self.render_option_list(false, cx),
                ));
            }
        }

        if self.kubernetes {
            rows.push(self.more_toggle(Section::Kubernetes, cx));
        }
        rows.push(self.heading("Environment", cx));
        rows.push(stacked_item(
            t,
            "Environment variables",
            note(t, "Set inside the VM, as KEY=value, one per row."),
            self.render_option_list(true, cx),
        ));

        if !creating {
            rows.push(self.heading("Configuration file", cx));
            rows.push(item(
                t,
                "colima.yaml",
                match self.editor_error.clone() {
                    Some(error) => warning(t, error),
                    None => note(
                        t,
                        "Registry mirrors, insecure registries and provision scripts. Changes apply on the next start.",
                    ),
                },
                Button::new("open-colima-yaml", "Open")
                    .on_click(cx.listener(|this, _, _, cx| this.open_config(cx)))
                    .into_any_element(),
            ));
        }
        rows
    }

    fn render_toggle(
        &self,
        id: &'static str,
        on: bool,
        toggle: fn(&mut Self),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // GPUI turns Space and Enter on the focused switch into a click, so no key handler here.
        Switch::new(id, on)
            .on_toggle(cx.listener(move |this, _, _, cx| {
                toggle(this);
                cx.notify();
            }))
            .into_any_element()
    }
}

impl Focusable for ProfileForm {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.mode {
            FormMode::Create { .. } => self.name.focus_handle(cx),
            FormMode::Edit { .. } => self.cpus.focus_handle(cx),
        }
    }
}

impl Render for ProfileForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let problem = self.problem(cx);
        let creating = self.creating();
        let (title, submit_label) = match &self.mode {
            FormMode::Create { .. } => ("New profile".to_owned(), "Create and start"),
            FormMode::Edit {
                profile,
                running: true,
                ..
            } => (format!("Edit {profile}"), "Save and restart"),
            FormMode::Edit {
                profile,
                running: false,
                ..
            } => (format!("Edit {profile}"), "Save and start"),
        };
        // Only nag about the name once the user has typed in it or tried to submit.
        let hide_problem = (creating && !self.name_touched && self.name.read(cx).text().is_empty())
            || problem.as_deref() == Some(NO_CHANGES);
        let options = self.options(cx);
        let rows = self.render_rows(&options, cx);

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_4()
                    .h_12()
                    .px_4()
                    .border_b_1()
                    .border_color(theme.border_variant)
                    .child(div().flex_none().text_lg().child(title))
                    // In the header so it stays visible while the form scrolls.
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .justify_end()
                            .text_sm()
                            .text_color(theme.error)
                            .when_some(problem.clone().filter(|_| !hide_problem), |el, problem| {
                                el.child(div().truncate().child(problem))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .gap_1()
                            .child(
                                Button::new("form-cancel", "Cancel")
                                    .size(ButtonSize::Medium)
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.emit(ProfileFormEvent::Cancelled)
                                    })),
                            )
                            .child(
                                Button::new("form-submit", submit_label)
                                    .style(ButtonStyle::Primary)
                                    .size(ButtonSize::Medium)
                                    .disabled(problem.is_some())
                                    .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                            ),
                    ),
            )
            .child(
                div().id("form-body").flex_1().overflow_y_scroll().child(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .max_w(px(720.))
                        .mx_auto()
                        .px_4()
                        .pb_8()
                        .when_some(self.load_warning.clone(), |el, warning| {
                            el.child(
                                div()
                                    .pt_4()
                                    .text_sm()
                                    .text_color(theme.warning)
                                    .child(warning),
                            )
                        })
                        .children(rows),
                ),
            )
    }
}

/// Title and description on the left, the control on the right (Zed's settings rows).
fn item(
    theme: &Theme,
    title: &'static str,
    description: AnyElement,
    control: AnyElement,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_4()
        .min_h(px(48.))
        .py_2()
        .border_b_1()
        .border_color(theme.border_variant)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(div().text_sm().child(title))
                .child(description),
        )
        .child(div().flex_none().child(control))
        .into_any_element()
}

/// Like [`item`], with a full-width body (lists) under the title.
fn stacked_item(
    theme: &Theme,
    title: &'static str,
    description: AnyElement,
    body: AnyElement,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .pt_2()
        .border_b_1()
        .border_color(theme.border_variant)
        .child(div().text_sm().child(title))
        .child(description)
        .child(body)
        .into_any_element()
}

fn note(theme: &Theme, text: &'static str) -> AnyElement {
    div()
        .text_xs()
        .text_color(theme.text_muted)
        .child(text)
        .into_any_element()
}

fn warning(theme: &Theme, text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_xs()
        .text_color(theme.warning)
        .child(text.into())
        .into_any_element()
}

/// Inputs, selects and steppers share one width.
fn field(control: AnyElement) -> AnyElement {
    div()
        // Selects and steppers are 180 px wide; inputs match them.
        .w(px(180.))
        .child(control)
        .into_any_element()
}

/// A value a profile can't change any more, in place of its control.
fn fixed(theme: &Theme, value: &str) -> AnyElement {
    div()
        .text_sm()
        .text_color(theme.text_muted)
        .child(value.to_owned())
        .into_any_element()
}

fn text_input(
    value: &str,
    placeholder: &'static str,
    cx: &mut Context<ProfileForm>,
) -> Entity<TextInput> {
    cx.new(|cx| {
        let mut input = TextInput::new(cx).placeholder(placeholder);
        input.set_text(value.to_owned(), cx);
        input
    })
}

/// Re-render on edits and submit the form on Enter.
fn subscribe_input(input: &Entity<TextInput>, cx: &mut Context<ProfileForm>) -> Subscription {
    cx.subscribe(input, |this, _, event: &TextInputEvent, cx| match event {
        TextInputEvent::Changed => cx.notify(),
        TextInputEvent::Submitted => this.submit(cx),
    })
}

fn option_row(
    id: usize,
    value: &str,
    placeholder: &'static str,
    cx: &mut Context<ProfileForm>,
) -> OptionRow {
    let input = text_input(value, placeholder, cx);
    OptionRow {
        id,
        _subscription: subscribe_input(&input, cx),
        input,
    }
}

fn nested_virtualization_supported() -> bool {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return false;
    }
    let Ok(chip) = crate::colima::cli::run("/usr/sbin/sysctl", &["-n", "machdep.cpu.brand_string"])
    else {
        return false;
    };
    let Ok(version) = crate::colima::cli::run("/usr/bin/sw_vers", &["-productVersion"]) else {
        return false;
    };
    supports_nested(
        &String::from_utf8_lossy(&chip.stdout),
        &String::from_utf8_lossy(&version.stdout),
    )
}

fn supports_nested(chip: &str, version: &str) -> bool {
    let generation = chip
        .split_whitespace()
        .find_map(|word| word.strip_prefix('M')?.parse::<u32>().ok());
    generation.is_some_and(|n| n >= 3)
        && version
            .trim()
            .split('.')
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .is_some_and(|v| v >= 15)
}

fn mount_row(id: usize, mount: &Mount, cx: &mut Context<ProfileForm>) -> MountRow {
    let path = text_input(&mount.location, "/path/to/folder or ~/folder", cx);
    MountRow {
        id,
        _subscription: subscribe_input(&path, cx),
        path,
        writable: mount.writable,
        mount_point: mount.mount_point.clone(),
    }
}

/// Comma-separated addresses; an empty field restores Colima's resolver.
fn dns_servers(text: &str) -> Vec<String> {
    if text.trim().is_empty() {
        Vec::new()
    } else {
        text.split(',')
            .map(|server| server.trim().to_owned())
            .collect()
    }
}

fn select(
    values: &[&'static str],
    initial: &str,
    cx: &mut Context<ProfileForm>,
) -> Entity<Select<&'static str>> {
    let options = values
        .iter()
        .map(|v| (*v, SharedString::from(*v)))
        .collect();
    cx.new(|cx| {
        let mut select = Select::new(options, cx);
        // An unknown saved value (e.g. a new colima option) falls back to the first entry.
        if let Some(value) = values.iter().find(|v| **v == initial) {
            select.set_value(value, cx);
        }
        select
    })
}

fn gib(bytes: u64) -> u32 {
    (bytes / (1 << 30)) as u32
}

/// Architecture name Colima uses for this host.
pub fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        _ => "aarch64",
    }
}

/// Profile names become directory names under `~/.colima` and part of the docker context name.
pub fn validate_name(name: &str, existing: &[String]) -> Result<(), String> {
    if name.is_empty() {
        return Err("Give the profile a name.".into());
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("Use lowercase letters, digits and dashes only.".into());
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err("Names can't start or end with a dash.".into());
    }
    if existing.iter().any(|e| e == name) {
        return Err(format!("A profile named “{name}” already exists."));
    }
    Ok(())
}

/// Combinations colima rejects at start time, caught before submitting.
pub fn validate_options(options: &StartOptions, host_arch: &str) -> Result<(), String> {
    if options.root_disk_gib == 0 {
        return Err("Root disk must be at least 1 GiB.".into());
    }
    if !options.hostname.is_empty()
        && (options.hostname.len() > 253
            || options.hostname.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            }))
    {
        return Err("Use a hostname made of letters, digits, dashes and dots.".into());
    }
    for (key, value) in &options.env {
        if key.is_empty()
            || !key.bytes().enumerate().all(|(index, b)| {
                b == b'_' || b.is_ascii_alphabetic() || (index > 0 && b.is_ascii_digit())
            })
            || value.chars().any(char::is_control)
        {
            return Err("Environment names need letters, digits or underscores and cannot start with a digit. Values cannot contain control characters.".into());
        }
    }
    if options
        .k3s_args
        .iter()
        .any(|arg| arg.is_empty() || arg.chars().any(char::is_control))
    {
        return Err("Give each k3s argument a value, or remove its row.".into());
    }
    // The characters `StartOptions::parse_config` accepts, so Edit can read it back.
    if !options
        .kubernetes_version
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-_.+".contains(&b))
    {
        return Err("Use a k3s version such as v1.35.0+k3s1, or leave it empty.".into());
    }
    if options.cpu_type.chars().any(char::is_control) {
        return Err("CPU type cannot contain control characters.".into());
    }
    if !matches!(options.port_forwarder.as_str(), "ssh" | "grpc" | "none") {
        return Err("Choose an available port forwarder.".into());
    }
    if options.arch != host_arch && options.vm_type != "qemu" {
        return Err(format!(
            "Emulating {} needs the qemu VM type.",
            options.arch
        ));
    }
    if options.vm_type == "krunkit" && host_arch != "aarch64" {
        return Err("krunkit needs an Apple silicon Mac.".into());
    }
    if options
        .dns
        .iter()
        .any(|server| server.parse::<std::net::IpAddr>().is_err())
    {
        return Err("DNS servers must be IP addresses separated by commas.".into());
    }
    if cfg!(target_os = "macos") {
        if !matches!(options.network_mode.as_str(), "shared" | "bridged") {
            return Err("Choose shared or bridged network mode.".into());
        }
        if options.network_mode == "bridged"
            && (options.network_interface.is_empty()
                || !options
                    .network_interface
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)))
        {
            return Err("Give the host interface a name, such as en0.".into());
        }
    }
    if let Some(mounts) = &options.mounts {
        for mount in mounts {
            let path = &mount.location;
            if !(path.starts_with('/') || path == "~" || path.starts_with("~/"))
                || path.contains(':')
                || path.chars().any(char::is_control)
            {
                return Err("Mount folders need an absolute path or ~/path, without colons or control characters.".into());
            }
        }
    }
    match (options.mount_type.as_str(), options.vm_type.as_str()) {
        ("virtiofs", "qemu") => Err("virtiofs mounts need the vz or krunkit VM type.".into()),
        ("9p", "vz" | "krunkit") => Err("9p mounts need the qemu VM type.".into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_virtualization_requires_m3_and_macos_15() {
        assert!(supports_nested("Apple M3 Pro", "15.0"));
        assert!(supports_nested("Apple M5 Max", "26.1"));
        for (chip, version) in [
            ("Apple M2 Max", "26.0"),
            ("Apple M3", "14.7"),
            ("Intel", "26.0"),
            ("", "15.0"),
            ("Apple M3", "unknown"),
        ] {
            assert!(!supports_nested(chip, version));
        }
    }

    #[test]
    fn advanced_option_validation() {
        let mut options = crate::colima::model::host_defaults("macos", "aarch64");
        for hostname in ["", "t65-vm", "t65-vm.example"] {
            options.hostname = hostname.into();
            assert!(validate_options(&options, "aarch64").is_ok());
        }
        for hostname in ["bad name", "-bad", "bad-", "bad..name", "bad;command"] {
            options.hostname = hostname.into();
            assert!(validate_options(&options, "aarch64").is_err());
        }
        options.hostname.clear();
        options
            .env
            .insert("T65_VALUE".into(), "comma, equals= and \"quote\"".into());
        assert!(validate_options(&options, "aarch64").is_ok());
        options.env.insert("1BAD".into(), "value".into());
        assert!(validate_options(&options, "aarch64").is_err());
        options.env.clear();
        options.k3s_args = vec!["".into()];
        assert!(validate_options(&options, "aarch64").is_err());
        options.k3s_args.clear();
        for version in ["", "v1.35.0+k3s1"] {
            options.kubernetes_version = version.into();
            assert!(validate_options(&options, "aarch64").is_ok(), "{version}");
        }
        for version in ["v1.35 0", "v1.35;rm", "v1/35"] {
            options.kubernetes_version = version.into();
            assert!(validate_options(&options, "aarch64").is_err(), "{version}");
        }
        options.kubernetes_version.clear();
        options.root_disk_gib = 0;
        assert!(validate_options(&options, "aarch64").is_err());
    }

    #[test]
    fn names() {
        let existing = vec!["default".to_owned()];
        assert!(validate_name("dev-2", &existing).is_ok());
        assert!(validate_name("", &existing).is_err());
        assert!(validate_name("Dev", &existing).is_err());
        assert!(validate_name("my_vm", &existing).is_err());
        assert!(validate_name("a b", &existing).is_err());
        assert!(validate_name("../x", &existing).is_err());
        assert!(validate_name("-x", &existing).is_err());
        assert!(validate_name("default", &existing).is_err());
    }

    #[test]
    fn network_options() {
        let mut options = crate::colima::model::host_defaults("macos", "aarch64");
        for input in ["", "1.1.1.1", "1.1.1.1, 2606:4700:4700::1111"] {
            options.dns = dns_servers(input);
            assert!(validate_options(&options, "aarch64").is_ok(), "{input}");
        }
        for input in [
            "example.com",
            "1.1.1.1,",
            ",1.1.1.1",
            "999.1.1.1",
            "1.1.1.1 8.8.8.8",
        ] {
            options.dns = dns_servers(input);
            assert!(validate_options(&options, "aarch64").is_err(), "{input}");
        }
        options.dns.clear();
        if cfg!(target_os = "macos") {
            options.network_mode = "bridged".into();
            for interface in ["en0", "bridge100", "bridge-test.1"] {
                options.network_interface = interface.into();
                assert!(validate_options(&options, "aarch64").is_ok());
            }
            for interface in ["", "en 0", "en0\n"] {
                options.network_interface = interface.into();
                assert!(validate_options(&options, "aarch64").is_err());
            }
        }
    }

    #[test]
    fn mount_paths() {
        let mut options = crate::colima::model::host_defaults("macos", "aarch64");
        for path in ["~", "~/projects", "/tmp/read only", "/tmp/comma,quote\""] {
            options.mounts = Some(vec![Mount {
                location: path.into(),
                writable: false,
                mount_point: String::new(),
            }]);
            assert!(validate_options(&options, "aarch64").is_ok(), "{path}");
        }
        for path in [
            "",
            "none",
            "relative/path",
            "/tmp/path:w",
            "/tmp/line\nfeed",
        ] {
            options.mounts = Some(vec![Mount {
                location: path.into(),
                writable: true,
                mount_point: String::new(),
            }]);
            assert!(validate_options(&options, "aarch64").is_err(), "{path}");
        }
        options.mounts = None;
        assert!(validate_options(&options, "aarch64").is_ok());
        options.mounts = Some(Vec::new());
        assert!(validate_options(&options, "aarch64").is_ok());
    }

    #[test]
    fn option_combinations() {
        let ok = StartOptions {
            arch: "aarch64".into(),
            ..crate::colima::model::host_defaults("macos", "aarch64")
        };
        assert!(validate_options(&ok, "aarch64").is_ok());

        let foreign = StartOptions {
            arch: "x86_64".into(),
            ..ok.clone()
        };
        assert!(validate_options(&foreign, "aarch64").is_err());
        let emulated = StartOptions {
            vm_type: "qemu".into(),
            mount_type: "sshfs".into(),
            ..foreign
        };
        assert!(validate_options(&emulated, "aarch64").is_ok());

        let qemu_virtiofs = StartOptions {
            vm_type: "qemu".into(),
            ..ok.clone()
        };
        assert!(validate_options(&qemu_virtiofs, "aarch64").is_err());
        let vz_9p = StartOptions {
            mount_type: "9p".into(),
            ..ok.clone()
        };
        assert!(validate_options(&vz_9p, "aarch64").is_err());
        let krunkit_intel = StartOptions {
            vm_type: "krunkit".into(),
            arch: "x86_64".into(),
            ..ok
        };
        assert!(validate_options(&krunkit_intel, "x86_64").is_err());
    }
}
