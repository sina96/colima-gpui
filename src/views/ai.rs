//! AI tab: the Colima agent skill and a ready-made prompt for this profile.

use std::path::{Path, PathBuf};

use gpui::{
    ClipboardItem, Context, Entity, PathPromptOptions, Subscription, Window, div, prelude::*, px,
};

use crate::ai::{prompt, skill};
use crate::settings;
use crate::store::ProfileStore;
use crate::theme::{ActiveTheme, MONO_FONT};
use crate::ui::dialog::{ConfirmDialog, DialogResult};
use crate::ui::{Button, ButtonStyle, Icon, IconName};

pub struct AiView {
    pub profile: String,
    store: Entity<ProfileStore>,
    busy: bool,
    /// The prompt preview is collapsed until asked for.
    show_preview: bool,
    result: Option<(String, bool)>,
    confirm: Option<(Entity<ConfirmDialog>, PathBuf, Subscription)>,
    _observe: Subscription,
}

impl AiView {
    pub fn new(profile: String, cx: &mut Context<Self>) -> Self {
        let store = ProfileStore::global(cx);
        let observe = cx.observe(&store, |_, _, cx| cx.notify());
        Self {
            profile,
            store,
            busy: false,
            show_preview: false,
            result: None,
            confirm: None,
            _observe: observe,
        }
    }

    fn prompt(&self, cx: &Context<Self>) -> Option<String> {
        let store = self.store.read(cx);
        let instance = store.instances().iter().find(|i| i.name == self.profile)?;
        let local = settings::get(cx)
            .skill_folder
            .as_ref()
            .map(|folder| folder.join("colima"))
            .filter(|path| path.join("SKILL.md").is_file());
        Some(prompt::build(
            instance,
            store.status(&self.profile),
            local.as_deref(),
        ))
    }

    fn pick_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.confirm.is_some() {
            return;
        }
        // This pinned GPUI API has no initial-directory parameter. Include the
        // remembered destination in the native prompt rather than changing cwd.
        let label = settings::get(cx).skill_folder.as_ref().map_or_else(
            || "Download skill here".to_owned(),
            |folder| format!("Download skill (last: {})", display_path(folder)),
        );
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(label.into()),
        });
        self.busy = true;
        self.result = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = selection.await;
            this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(folder) = paths.into_iter().next() {
                            this.choose_folder(folder, window, cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.result = Some((error.to_string(), true)),
                    Err(error) => this.result = Some((error.to_string(), true)),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn choose_folder(&mut self, folder: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let destination = folder.join("colima");
        if destination.symlink_metadata().is_err() {
            self.download(folder, false, cx);
            return;
        }
        let dialog = cx.new(|cx| {
            ConfirmDialog::new(
                "Replace Colima skill?",
                format!(
                    "Replace {} and its contents with a fresh download?",
                    display_path(&destination)
                ),
                cx,
            )
            .confirm_label("Replace")
        });
        let subscription = cx.subscribe(&dialog, |this, _, result, cx| {
            if let Some((_, folder, _)) = this.confirm.take()
                && *result == DialogResult::Confirmed
            {
                this.download(folder, true, cx);
            }
            cx.notify();
        });
        dialog.update(cx, |dialog, cx| dialog.show(window, cx));
        self.confirm = Some((dialog, folder, subscription));
        cx.notify();
    }

    fn download(&mut self, folder: PathBuf, replace: bool, cx: &mut Context<Self>) {
        self.busy = true;
        self.result = None;
        cx.notify();
        let target = folder.clone();
        let download = cx
            .background_executor()
            .spawn(async move { skill::download(&target, replace) });
        cx.spawn(async move |this, cx| {
            let result = download.await;
            this.update(cx, |this, cx| {
                this.busy = false;
                this.result = Some(match result {
                    Ok(path) => {
                        settings::update(cx, |settings| settings.skill_folder = Some(folder));
                        (format!("Saved to {}", display_path(&path)), false)
                    }
                    Err(error) => (format!("Download failed: {error}"), true),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn display_path(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME")
        && let Ok(relative) = path.strip_prefix(Path::new(&home))
    {
        return format!("~/{}", relative.display());
    }
    path.display().to_string()
}

impl Render for AiView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let prompt = self.prompt(cx);
        let can_copy = prompt.is_some();
        div()
            .id("ai-tab")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .bg(theme.editor_background)
            .text_sm()
            .text_color(theme.text)
            .child("Colima agent skill")
            .child(div().text_color(theme.text_muted).child(
                "Give your coding agent the Colima skill and a prompt with this profile’s current details.",
            ))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("view-skill", "View on GitHub")
                    .icon(IconName::ArrowUpRight)
                    .on_click(|_, _, cx| cx.open_url(skill::GITHUB_URL)))
                .child(Button::new("download-skill", "Download skill…")
                    .icon(IconName::Download)
                    .loading(self.busy)
                    .disabled(self.confirm.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.pick_folder(window, cx))))
                .child(Button::new("copy-ai-prompt", "Copy prompt")
                    .style(ButtonStyle::Primary)
                    .icon(IconName::Copy)
                    .disabled(!can_copy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(prompt) = this.prompt(cx) {
                            cx.write_to_clipboard(ClipboardItem::new_string(prompt));
                            this.result = Some(("Prompt copied".into(), false));
                            cx.notify();
                        }
                    }))))
            .when_some(self.result.clone(), |el, (message, error)| {
                el.child(div().text_color(if error { theme.error } else { theme.success }).child(message))
            })
            .child(div()
                .id("ai-preview-toggle")
                .flex()
                .items_center()
                .gap_1()
                .w_full()
                .h(px(22.))
                .px_1()
                .rounded_sm()
                .cursor_pointer()
                .text_color(theme.text_muted)
                .hover(|s| s.bg(theme.ghost_element_hover).text_color(theme.text))
                .child(Icon::new(if self.show_preview { IconName::ChevronDown } else { IconName::ChevronRight }).size(px(14.)))
                .child("Prompt preview")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_preview = !this.show_preview;
                    cx.notify();
                })))
            .when(self.show_preview, |el| el.child(div()
                .w_full()
                .p_3()
                .rounded_sm()
                .border_1()
                .border_color(theme.border_variant)
                .bg(theme.surface_background)
                .font_family(MONO_FONT)
                .child(prompt.unwrap_or_else(|| "Profile details are not available yet.".into()))))
            .when_some(self.confirm.as_ref(), |el, (dialog, _, _)| el.child(dialog.clone()))
    }
}
