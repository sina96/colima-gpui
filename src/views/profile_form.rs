//! Create/edit profile form producing [`StartOptions`].
//!
//! Create mode exposes every option. Edit mode only offers what colima can change
//! on an existing VM (CPUs, memory, growing the disk, mounts, Kubernetes, network address);
//! runtime, VM type, architecture and mount type are fixed after creation.

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::colima::Instance;
use crate::colima::model::{Mount, StartOptions};
use crate::theme::ActiveTheme;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileFormEvent {
    Cancelled,
    Submitted {
        profile: String,
        options: StartOptions,
        stop_first: bool,
    },
}

struct MountRow {
    id: usize,
    path: Entity<TextInput>,
    writable: bool,
    mount_point: String,
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
    no_mounts: bool,
    next_mount_id: usize,
    kubernetes: bool,
    kubernetes_version: Entity<TextInput>,
    rosetta: bool,
    advanced_open: bool,
    network_address: bool,
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
                        "Couldn't read colima.yaml ({e}); showing defaults for the other settings."
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
        initial: StartOptions,
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
        let kubernetes_version = cx.new(|cx| {
            let mut input = TextInput::new(cx).placeholder("Colima default");
            input.set_text(initial.kubernetes_version.clone(), cx);
            input
        });

        let mounts: Vec<_> = initial
            .mounts
            .as_deref()
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(id, mount)| mount_row(id, mount, cx))
            .collect();
        let next_mount_id = mounts.len();
        let subscriptions = vec![
            cx.subscribe(&name, |this, _, event: &TextInputEvent, cx| match event {
                TextInputEvent::Changed => {
                    this.name_touched = true;
                    cx.notify();
                }
                TextInputEvent::Submitted => this.submit(cx),
            }),
            cx.subscribe(&cpus, |_, _, _: &StepperChanged, cx| cx.notify()),
            cx.subscribe(&memory, |_, _, _: &StepperChanged, cx| cx.notify()),
            cx.subscribe(&disk, |_, _, _: &StepperChanged, cx| cx.notify()),
            cx.subscribe(&runtime, |_, _, _: &SelectChanged<&'static str>, cx| {
                cx.notify()
            }),
            cx.subscribe(&vm_type, |_, _, _: &SelectChanged<&'static str>, cx| {
                cx.notify()
            }),
            cx.subscribe(&arch, |_, _, _: &SelectChanged<&'static str>, cx| {
                cx.notify()
            }),
            cx.subscribe(&mount_type, |_, _, _: &SelectChanged<&'static str>, cx| {
                cx.notify()
            }),
            cx.subscribe(
                &kubernetes_version,
                |this, _, event: &TextInputEvent, cx| match event {
                    TextInputEvent::Changed => cx.notify(),
                    TextInputEvent::Submitted => this.submit(cx),
                },
            ),
        ];

        Self {
            mode,
            no_mounts: initial.mounts.is_none(),
            mounts,
            next_mount_id,
            kubernetes: initial.kubernetes,
            kubernetes_version,
            rosetta: initial.rosetta,
            advanced_open: false,
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

    fn options(&self, cx: &App) -> StartOptions {
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
            mounts: (!self.no_mounts).then(|| {
                self.mounts
                    .iter()
                    .map(|row| Mount {
                        location: row.path.read(cx).text().trim().to_owned(),
                        writable: row.writable,
                        mount_point: row.mount_point.clone(),
                    })
                    .collect()
            }),
            network_address: self.network_address,
            activate: self.activate,
        }
    }

    fn profile_name(&self, cx: &App) -> String {
        match &self.mode {
            FormMode::Create { .. } => self.name.read(cx).text().trim().to_owned(),
            FormMode::Edit { profile, .. } => profile.clone(),
        }
    }

    /// The first problem blocking submit, if any.
    fn problem(&self, cx: &App) -> Option<String> {
        let options = self.options(cx);
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
                    Some("No changes yet.".into())
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

    fn render_mounts(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_muted)
                            .child("No mounts"),
                    )
                    .child(self.render_toggle(
                        "no-mounts",
                        self.no_mounts,
                        |this| this.no_mounts = !this.no_mounts,
                        cx,
                    )),
            )
            .when(!self.no_mounts, |el| {
                el.children(self.mounts.iter().map(|row| {
                    let id = row.id;
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(row.path.clone()))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.text_muted)
                                        .child("Writable"),
                                )
                                .child(
                                    Switch::new(("mount-writable", id), row.writable).on_toggle(
                                        cx.listener(move |this, _, _, cx| {
                                            if let Some(row) =
                                                this.mounts.iter_mut().find(|row| row.id == id)
                                            {
                                                row.writable = !row.writable;
                                            }
                                            cx.notify();
                                        }),
                                    ),
                                ),
                        )
                        .child(
                            Button::new(("mount-remove", id), "Remove").on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.mounts.retain(|row| row.id != id);
                                    cx.notify();
                                },
                            )),
                        )
                        .when(!row.mount_point.is_empty(), |el| {
                            el.child(InfoTip::new(
                                ("mount-point", id),
                                format!("Guest path: {}", row.mount_point),
                            ))
                        })
                }))
                .child(
                    div()
                        .flex()
                        .child(Button::new("mount-add", "Add folder").on_click(cx.listener(
                            |this, _, window, cx| {
                                let row = mount_row(
                                    this.next_mount_id,
                                    &Mount {
                                        location: String::new(),
                                        writable: true,
                                        mount_point: String::new(),
                                    },
                                    cx,
                                );
                                this.next_mount_id += 1;
                                window.focus(&row.path.focus_handle(cx), cx);
                                this.mounts.push(row);
                                cx.notify();
                            },
                        ))),
                )
                .when(self.mounts.is_empty(), |el| {
                    el.child(div().text_sm().text_color(theme.text_muted).child(
                        "Default: home folder, writable. Adding folders replaces the default.",
                    ))
                })
            })
            .into_any_element()
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
        let creating = matches!(self.mode, FormMode::Create { .. });
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
        let hide_problem = creating && !self.name_touched && self.name.read(cx).text().is_empty();
        let options = self.options(cx);

        let fixed = |value: &str| div().text_sm().child(value.to_owned()).into_any_element();
        let mut rows: Vec<(&str, AnyElement)> = Vec::new();
        if creating {
            rows.push(("Name", self.name.clone().into_any_element()));
        }
        rows.push(("CPUs", self.cpus.clone().into_any_element()));
        rows.push(("Memory", self.memory.clone().into_any_element()));
        rows.push(("Disk", self.disk.clone().into_any_element()));
        if creating {
            rows.push(("Runtime", self.runtime.clone().into_any_element()));
            rows.push(("VM type", self.vm_type.clone().into_any_element()));
            rows.push(("Architecture", self.arch.clone().into_any_element()));
            rows.push(("Mount type", self.mount_type.clone().into_any_element()));
        } else {
            rows.push(("Runtime", fixed(&options.runtime)));
            rows.push(("VM type", fixed(&options.vm_type)));
            rows.push(("Architecture", fixed(&options.arch)));
            rows.push(("Mount type", fixed(&options.mount_type)));
        }
        // Mount folders remain in the main form in both create and edit modes.
        rows.push(("Mount folders", self.render_mounts(cx)));
        rows.push((
            "Kubernetes",
            self.render_toggle(
                "kubernetes",
                self.kubernetes,
                |this| this.kubernetes = !this.kubernetes,
                cx,
            ),
        ));
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) && options.vm_type == "vz" {
            rows.push((
                "Rosetta",
                self.render_toggle(
                    "rosetta",
                    self.rosetta,
                    |this| this.rosetta = !this.rosetta,
                    cx,
                ),
            ));
        }
        rows.push((
            "Network address",
            self.render_toggle(
                "network-address",
                self.network_address,
                |this| this.network_address = !this.network_address,
                cx,
            ),
        ));
        rows.push((
            "Make active Docker context",
            self.render_toggle(
                "activate",
                self.activate,
                |this| this.activate = !this.activate,
                cx,
            ),
        ));

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .h_12()
                    .px_4()
                    .border_b_1()
                    .border_color(theme.border_variant)
                    .child(div().text_lg().child(title))
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                Button::new("form-cancel", "Cancel")
                                    .size(ButtonSize::Medium)
                                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ProfileFormEvent::Cancelled))),
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
                div()
                    .id("form-body")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_3()
                    .p_4()
                    .overflow_y_scroll()
                    .when_some(self.load_warning.clone(), |el, warning| {
                        el.child(div().text_sm().text_color(theme.warning).child(warning))
                    })
                    .child(div().flex().flex_col().children(rows.into_iter().map(|(label, control)| {
                        div()
                            .flex()
                            .items_center()
                            .min_h(px(40.))
                            .when(label == "Mount folders", |el| el.py_2())
                            .border_b_1()
                            .border_color(theme.border_variant)
                            .child(
                                div()
                                    .w(px(200.))
                                    .flex_none()
                                    .text_sm()
                                    .text_color(theme.text_muted)
                                    .child(label.to_owned()),
                            )
                            .child(div().flex_1().min_w_0().max_w(px(if label == "Mount folders" { 520. } else { 320. })).child(control))
                    })))
                    .child(
                        div().flex().child(
                            Button::new("advanced-options", "Advanced")
                                .icon(if self.advanced_open { IconName::ChevronDown } else { IconName::ChevronRight })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.advanced_open = !this.advanced_open;
                                    cx.notify();
                                })),
                        ),
                    )
                    .when(self.advanced_open, |el| {
                        // Keep the group order stable as more Colima options are added.
                        el.child(
                            div().flex().flex_col().gap_3()
                                .child(advanced_heading("Mounts", cx))
                                .child(advanced_heading("Network", cx))
                                .child(advanced_heading("Kubernetes", cx))
                                .when(self.kubernetes, |el| {
                                    el.child(
                                        div().flex().items_center().min_h(px(40.))
                                            .child(div().w(px(200.)).flex_none().text_sm()
                                                .text_color(theme.text_muted).child("Version"))
                                            .child(div().flex_1().max_w(px(320.)).child(self.kubernetes_version.clone())),
                                    )
                                })
                                .child(advanced_heading("VM", cx)),
                        )
                    })
                    .when_some(problem.filter(|_| !hide_problem), |el, problem| {
                        el.child(div().text_sm().text_color(theme.error).child(problem))
                    })
                    .when(!creating, |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_sm()
                                .text_color(theme.text_muted)
                                .child("Editing limits")
                                .child(InfoTip::new(
                                    "editing-limits-info",
                                    "Disk can only grow. Runtime, VM type, architecture and mount type are fixed once a profile exists; delete and recreate it to change them.",
                                )),
                        )
                    }),
            )
    }
}

fn mount_row(id: usize, mount: &Mount, cx: &mut Context<ProfileForm>) -> MountRow {
    let path = cx.new(|cx| {
        let mut input = TextInput::new(cx).placeholder("/path/to/folder or ~/folder");
        input.set_text(mount.location.clone(), cx);
        input
    });
    let subscription = cx.subscribe(&path, |this, _, event: &TextInputEvent, cx| match event {
        TextInputEvent::Changed => cx.notify(),
        TextInputEvent::Submitted => this.submit(cx),
    });
    MountRow {
        id,
        path,
        writable: mount.writable,
        mount_point: mount.mount_point.clone(),
        _subscription: subscription,
    }
}

fn advanced_heading(label: &'static str, cx: &App) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(cx.theme().text_muted)
        .border_b_1()
        .border_color(cx.theme().border_variant)
        .pb_1()
        .child(label)
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
    if options.arch != host_arch && options.vm_type != "qemu" {
        return Err(format!(
            "Emulating {} needs the qemu VM type.",
            options.arch
        ));
    }
    if options.vm_type == "krunkit" && host_arch != "aarch64" {
        return Err("krunkit needs an Apple silicon Mac.".into());
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
