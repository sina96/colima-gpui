use colima_gpui::colima::commands::LogLine;
use colima_gpui::ui::select::{Select, SelectChanged};
use colima_gpui::ui::stepper::{Stepper, StepperChanged};
use colima_gpui::ui::text_input::{TextInput, TextInputEvent};
use colima_gpui::ui::{Column, Table, TextCell, size_cell};
use colima_gpui::views::log_panel::LogPanel;
use colima_gpui::{
    assets::Assets,
    theme::{
        self, ActiveTheme, CHART_BUILD_CACHE, CHART_CONTAINERS, CHART_IMAGES, CHART_VOLUMES,
        UI_FONT,
    },
    ui::{
        self, Button, ButtonStyle, CardGrid, DetailCard, DetailPage, Field, Icon, IconButton,
        IconName, InfoTip, Link, MenuButton, MenuItem, Meter, Segment, SizeBar, StackedBar,
        StatusDot, Switch,
        badge::Badge,
        button::ButtonSize,
        card::Card,
        dialog::{ConfirmDialog, DialogResult},
    },
};
use gpui::{
    App, Context, Entity, FocusHandle, Subscription, Window, WindowOptions, div, prelude::*, px,
};
use gpui_platform::application;
use std::rc::Rc;
struct Gallery {
    dialog: Entity<ConfirmDialog>,
    message: String,
    /// Shows the DetailPage demo in place of the gallery.
    detail_open: bool,
    detail_tab: usize,
    detail_focus: FocusHandle,
    log: Entity<LogPanel>,
    selected_row: Option<usize>,
    switch_on: bool,
    name: Entity<TextInput>,
    cpus: Entity<TextInput>,
    runtime: Entity<Select<String>>,
    memory: Entity<Stepper>,
    _subscriptions: Vec<Subscription>,
}
impl Gallery {
    fn new(cx: &mut Context<Self>) -> Self {
        let dialog = cx.new(|cx| {
            ConfirmDialog::new(
                "Delete profile?",
                "This gallery only demonstrates confirmation.",
                cx,
            )
        });
        let name = cx.new(|cx| TextInput::new(cx).placeholder("Profile name"));
        let cpus = cx.new(|cx| {
            let mut input = TextInput::new(cx).placeholder("CPUs").numeric();
            input.set_text("2", cx);
            input
        });
        let runtime = cx.new(|cx| {
            Select::new(
                ["docker", "containerd", "incus"]
                    .into_iter()
                    .map(|s| (s.to_owned(), s.into()))
                    .collect(),
                cx,
            )
        });
        let memory = cx.new(|cx| Stepper::new(2, 1, 8, 1, cx).unit("GiB"));
        let subscriptions = vec![
            cx.subscribe(&dialog, |this, _, result, cx| {
                this.say(
                    match result {
                        DialogResult::Confirmed => "Confirmed",
                        DialogResult::Cancelled => "Cancelled",
                    },
                    cx,
                )
            }),
            cx.subscribe(&name, |this, _, event: &TextInputEvent, cx| {
                this.say(&format!("Name: {event:?}"), cx)
            }),
            cx.subscribe(&cpus, |this, _, event: &TextInputEvent, cx| {
                this.say(&format!("CPUs: {event:?}"), cx)
            }),
            cx.subscribe(&runtime, |this, _, event: &SelectChanged<String>, cx| {
                this.say(&format!("Runtime changed: {}", event.0), cx)
            }),
            cx.subscribe(&memory, |this, _, event: &StepperChanged, cx| {
                this.say(&format!("Memory changed: {} GiB", event.0), cx)
            }),
        ];
        Self {
            dialog,
            message: "Ready".into(),
            detail_open: false,
            detail_tab: 0,
            detail_focus: cx.focus_handle(),
            log: cx.new(|cx| LogPanel::new(fake_log_stream(), cx).plain_stderr()),
            selected_row: None,
            switch_on: true,
            name,
            cpus,
            runtime,
            memory,
            _subscriptions: subscriptions,
        }
    }
}
impl Gallery {
    fn say(&mut self, message: &str, cx: &mut Context<Self>) {
        self.message = message.into();
        cx.notify();
    }

    fn render_polish(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = cx.theme().clone();
        let gb = |gb: f64| gb * 1e9;
        let size = |gb: f64| format!("{gb:.1} GB");
        let segments = vec![
            Segment {
                label: "Images".into(),
                value_label: size(20.7).into(),
                value: gb(20.7),
                reclaimable: gb(4.1),
                color: t.chart[CHART_IMAGES],
            },
            Segment {
                label: "Containers".into(),
                value_label: size(10.0).into(),
                value: gb(10.0),
                reclaimable: gb(9.3),
                color: t.chart[CHART_CONTAINERS],
            },
            Segment {
                label: "Volumes".into(),
                value_label: size(0.05).into(),
                value: gb(0.05),
                reclaimable: 0.,
                color: t.chart[CHART_VOLUMES],
            },
            Segment {
                label: "Build cache".into(),
                value_label: size(1.2).into(),
                value: gb(1.2),
                reclaimable: gb(1.2),
                color: t.chart[CHART_BUILD_CACHE],
            },
        ];
        let containers = [
            ("running", "Up 2 hours"),
            ("paused", "Up 1 hour (Paused)"),
            ("exited", "Exited (0) 3 minutes ago"),
            ("exited", "Exited (137) 1 hour ago"),
            ("restarting", "Restarting (1) 5 seconds ago"),
            ("created", "Created"),
        ];
        let rows = [
            ("grafana/otel-lgtm:latest", 3.49),
            ("postgres:16", 0.45),
            ("hello-world", 0.01),
        ];

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(Card::new(
                "Icons (Lucide, 1.5 px stroke)",
                div()
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .children(IconName::ALL.map(Icon::new)),
            ))
            .child(Card::new(
                "Status dots: hover for the state",
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child("Containers")
                    .children(containers.iter().enumerate().map(|(i, (state, status))| {
                        StatusDot::for_container(("c", i), state, status)
                    }))
                    .child("Images")
                    .child(StatusDot::for_image("img-used", true, false))
                    .child(StatusDot::for_image("img-unused", false, false))
                    .child(StatusDot::for_image("img-dangling", false, true))
                    .child("Volumes")
                    .child(StatusDot::for_volume("vol-used", true))
                    .child(StatusDot::for_volume("vol-unused", false)),
            ))
            .child(Card::new(
                "Detail cards: CardGrid, DetailCard, Field, Link (resize to see 1–2 columns)",
                CardGrid::new()
                    .card(
                        DetailCard::new(IconName::Box, "Summary")
                            .child(Field::new("Status", "running · Up 18 hours (healthy)"))
                            .child(Field::new("Image", "ministackorg/ministack:1.5.28").link(
                                "gallery-image-link",
                                cx.listener(|this, _, _, cx| this.say("Image link clicked", cx)),
                            ))
                            .child(Field::new("Created", "2026-10-17 17:55")),
                    )
                    .card(
                        DetailCard::new(IconName::Terminal, "Command")
                            .child(
                                Field::new("ID", "da327c9c1beca6875b24fb3ae1fccfeccdd4bc6aca27504728e21cdca42502b3")
                                    .mono()
                                    .copyable(),
                            )
                            .child(Field::new("Entrypoint", "python -m ministack").mono())
                            .child(Field::new("Cmd", "none").muted()),
                    )
                    .card(DetailCard::new(IconName::Network, "Ports").empty("No published ports"))
                    .wide(
                        DetailCard::new(IconName::FileText, "Environment")
                            .action(
                                Link::new("gallery-link", "Open volume").on_click(
                                    cx.listener(|this, _, _, cx| this.say("Link clicked", cx)),
                                ),
                            )
                            .child(Field::new("PATH", "/usr/local/bin:/usr/bin:/bin").mono().wrap()),
                    ),
            ))
            .child(Card::new(
                "Switch (on, off, disabled)",
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(Switch::new("gallery-switch", self.switch_on).on_toggle(cx.listener(
                        |this, on: &bool, _, cx| {
                            this.switch_on = *on;
                            this.say(if *on { "Switched on" } else { "Switched off" }, cx);
                        },
                    )))
                    .child(Switch::new("gallery-switch-off", false).on_toggle(|_, _, _| {}))
                    .child(Switch::new("gallery-switch-disabled", true).disabled(true)),
            ))
            .child(Card::new(
                "InfoTip, IconButton, MenuButton",
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child("Reclaimable")
                    .child(InfoTip::new(
                        "tip",
                        "Space Docker can free without touching anything in use.",
                    ))
                    .child(
                        IconButton::new("copy", IconName::Copy)
                            .tooltip("Copy")
                            .on_click(cx.listener(|this, _, _, cx| this.say("Copied", cx))),
                    )
                    .child(
                        IconButton::new("delete", IconName::Trash)
                            .tooltip("Delete")
                            .danger()
                            .on_click(cx.listener(|this, _, _, cx| this.say("Delete clicked", cx))),
                    )
                    .child(
                        IconButton::new("off", IconName::Play)
                            .tooltip("Disabled")
                            .disabled(true),
                    )
                    .child(
                        MenuButton::new("menu")
                            .item(
                                MenuItem::new(
                                    "Restart",
                                    cx.listener(|this, _, _, cx| this.say("Restart", cx)),
                                )
                                .icon(IconName::RotateCw),
                            )
                            .item(
                                MenuItem::new(
                                    "Logs",
                                    cx.listener(|this, _, _, cx| this.say("Logs", cx)),
                                )
                                .icon(IconName::FileText),
                            )
                            .item(MenuItem::new("Disabled", |_, _, _| {}).disabled(true))
                            .item(
                                MenuItem::new(
                                    "Remove",
                                    cx.listener(|this, _, _, cx| this.say("Remove", cx)),
                                )
                                .icon(IconName::Trash)
                                .danger(),
                            ),
                    ),
            ))
            .child(Card::new("StackedBar", StackedBar::new(segments)))
            .child(Card::new(
                "Table: columns stay aligned at any width; long names truncate (hover for full)",
                div().h(px(200.)).child(self.render_table(cx)),
            ))
            .child(Card::new(
                "LogPanel: terminal style, live (scroll up pauses Follow)",
                div().h(px(220.)).child(self.log.clone()),
            ))
            .child(Card::new(
                "SizeBar and Meter",
                div()
                    .flex()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .w(px(260.))
                            .children(rows.iter().map(|(name, size_gb)| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().w(px(150.)).truncate().child(*name))
                                    .child(div().w(px(60.)).child(SizeBar::new(*size_gb, 3.49)))
                                    .child(div().text_xs().child(size(*size_gb)))
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .w(px(220.))
                            .child(Meter::new("Memory", "3.1 of 7.7 GiB", 3.1, 7.7))
                            .child(Meter::new("Load", "6.3 of 8 CPUs", 6.3, 8.))
                            .child(Meter::new("Disk", "18.9 of 20 GB", 18.9, 20.)),
                    ),
            ))
            .child(
                Button::new("open-detail", "Open DetailPage demo").on_click(cx.listener(
                    |this, _, window, cx| {
                        this.detail_open = true;
                        window.focus(&this.detail_focus, cx);
                        cx.notify();
                    },
                )),
            )
    }

    fn render_table(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rows: Rc<Vec<DemoRow>> = Rc::new(vec![
            (
                "grafana/otel-lgtm:latest",
                "3.49 GB",
                3.49,
                "2026-10-16",
                false,
            ),
            (
                "mcr.microsoft.com/mssql/server:2022-latest-with-a-very-long-tag",
                "2.34 GB",
                2.34,
                "2026-07-08",
                true,
            ),
            ("postgres:18-alpine", "425 MB", 0.425, "2026-09-17", true),
            ("hello-world:latest", "22.6 kB", 0.00002, "2026-03-23", true),
        ]);
        let view = cx.entity().downgrade();
        Table::new(
            "gallery-table",
            vec![
                Column::fixed("", px(16.)),
                Column::flex("Repository:tag", 1.).min(px(140.)),
                Column::fixed("Size", px(110.))
                    .right()
                    .info("Unique size on disk"),
                Column::fixed("Created", px(100.)),
                Column::actions(),
            ],
            rows.len(),
            {
                let rows = rows.clone();
                move |ix, _, _| {
                    let (name, size_label, size, created, in_use) = rows[ix];
                    vec![
                        StatusDot::for_image(("t-status", ix), in_use, false).into_any_element(),
                        TextCell::new(("t-name", ix), name).into_any_element(),
                        size_cell(size_label, size, 3.49).into_any_element(),
                        TextCell::new(("t-created", ix), created)
                            .muted()
                            .into_any_element(),
                        MenuButton::new(("t-menu", ix))
                            .item(
                                MenuItem::new("Remove", |_, _, _| {})
                                    .icon(IconName::Trash)
                                    .danger(),
                            )
                            .into_any_element(),
                    ]
                }
            },
        )
        .selected(self.selected_row)
        .on_row_click(move |ix, _, cx| {
            view.update(cx, |this, cx| {
                this.selected_row = Some(ix);
                this.say(&format!("Row {ix} clicked"), cx);
            })
            .ok();
        })
    }

    fn render_detail(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let tabs = ["Logs", "Inspect", "Stats"];
        let mut page = DetailPage::new("demo-grafana")
            .status(StatusDot::for_container(
                "detail-status",
                "running",
                "Up 2 hours",
            ))
            .subtitle("grafana/otel-lgtm:0.36.0")
            .action(
                IconButton::new("d-restart", IconName::RotateCw)
                    .tooltip("Restart")
                    .medium()
                    .on_click(|_, _, _| {}),
            )
            .action(
                IconButton::new("d-remove", IconName::Trash)
                    .tooltip("Remove")
                    .danger()
                    .medium()
                    .on_click(|_, _, _| {}),
            )
            .track_focus(&self.detail_focus)
            .on_back({
                let this = cx.entity().downgrade();
                move |_, cx| {
                    this.update(cx, |this, cx| {
                        this.detail_open = false;
                        cx.notify();
                    })
                    .ok();
                }
            })
            .body(div().p_4().child(format!(
                "{} tab body. Esc or ← Back returns.",
                tabs[self.detail_tab]
            )));
        for (i, label) in tabs.into_iter().enumerate() {
            page = page.tab(
                label,
                i == self.detail_tab,
                cx.listener(move |this, _, _, cx| {
                    this.detail_tab = i;
                    cx.notify();
                }),
            );
        }
        page
    }
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.detail_open {
            let page = self.render_detail(cx);
            let t = cx.theme();
            return div()
                .size_full()
                .font_family(UI_FONT)
                .text_sm()
                .bg(t.editor_background)
                .text_color(t.text)
                .child(page)
                .into_any_element();
        }
        let polish = self.render_polish(cx);
        let t = cx.theme();
        let buttons = [
            ButtonStyle::Primary,
            ButtonStyle::Secondary,
            ButtonStyle::Danger,
            ButtonStyle::Ghost,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, style)| {
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new(("normal", i), format!("{style:?}"))
                        .style(style)
                        .tooltip("A reusable button")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.message = "Clicked".into();
                            cx.notify();
                        })),
                )
                .child(
                    Button::new(("disabled", i), "Disabled")
                        .style(style)
                        .disabled(true),
                )
                .child(
                    Button::new(("loading", i), "Start")
                        .style(style)
                        .loading(true),
                )
        });
        div()
            .id("gallery")
            .size_full()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .font_family(UI_FONT)
            .text_sm()
            .bg(t.editor_background)
            .text_color(t.text)
            .child("UI gallery · Tab / Shift-Tab to focus, Space / Enter to activate")
            .child(Card::new(
                "Buttons",
                div().flex().flex_col().gap_2().children(buttons),
            ))
            .child(Card::new(
                "Form controls: TextInput (text, numeric), Select, Stepper · type, copy/paste, Up/Down, Enter, Escape, Home/End",
                div()
                    .flex()
                    .flex_wrap()
                    .items_end()
                    .gap_4()
                    .child(labeled("Name", t.text_muted, div().w(px(200.)).child(self.name.clone())))
                    .child(labeled("CPUs (numeric)", t.text_muted, div().w(px(80.)).child(self.cpus.clone())))
                    .child(labeled("Runtime", t.text_muted, self.runtime.clone()))
                    .child(labeled("Memory (1–8 GiB)", t.text_muted, self.memory.clone())),
            ))
            .child(
                div().flex().gap_2().children(
                    [ButtonSize::Default, ButtonSize::Medium, ButtonSize::Large]
                        .into_iter()
                        .enumerate()
                        .map(|(i, size)| {
                            Button::new(("size", i), format!("{size:?}"))
                                .size(size)
                                .on_click(|_, _, _| {})
                        }),
                ),
            )
            .child(
                div().flex().gap_3().children(
                    ["Running", "Stopped", "Broken", "Unknown"]
                        .into_iter()
                        .map(Badge::status),
                ),
            )
            .child(
                Button::new("open-dialog", "Open confirmation").on_click(cx.listener(
                    |this, _, window, cx| {
                        this.dialog.update(cx, |dialog, cx| dialog.show(window, cx))
                    },
                )),
            )
            .child(self.message.clone())
            .child(polish)
            .child(self.dialog.clone())
            .on_key_down(|event, window, cx| {
                if event.keystroke.key == "tab" {
                    if event.keystroke.modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    cx.stop_propagation();
                }
            })
            .into_any_element()
    }
}
/// A small muted caption above a control.
fn labeled(label: &'static str, color: gpui::Hsla, control: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_xs().text_color(color).child(label))
        .child(control)
}

/// (name, size label, size in GB, created, in use)
type DemoRow = (&'static str, &'static str, f64, &'static str, bool);

/// Emits a log line every 400 ms until the panel drops the receiver.
fn fake_log_stream() -> std::sync::mpsc::Receiver<LogLine> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for i in 0.. {
            let line = if i % 5 == 4 {
                LogLine::Stderr(format!(
                    "{i:04} stderr is shown as normal output for containers"
                ))
            } else {
                LogLine::Stdout(format!(
                    "{i:04} GET /api/health 200 1.2ms - a long line that wraps when Wrap is on, scrolls sideways otherwise"
                ))
            };
            if tx.send(line).is_err() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
    });
    rx
}

fn main() {
    application().with_assets(Assets).run(|cx: &mut App| {
        theme::init(cx);
        ui::init(cx);
        cx.open_window(WindowOptions::default(), |_, cx| cx.new(Gallery::new))
            .unwrap();
        cx.activate(true);
    });
}
