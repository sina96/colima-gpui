//! What the menu bar menu shows and what its items do, kept free of AppKit so it's tested.
//!
//! Each profile is a submenu titled "default — Running" with a status dot; its items start,
//! stop or restart it, or open it in the window. Menu item IDs encode a [`Command`].

use colima_gpui::colima::Instance;
use colima_gpui::store::Op;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    Stopped,
    /// A start, stop, restart or delete is in progress.
    Busy,
    /// The last operation failed.
    Failed,
}

/// One profile as the menu shows it. Rebuilding the menu only when these change keeps an
/// open menu from being replaced on every poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub state: State,
    /// "default — Running", or "default — Starting…" while busy.
    pub title: String,
}

pub fn rows(
    instances: &[Instance],
    busy: impl Fn(&str) -> Option<Op>,
    failed: impl Fn(&str) -> bool,
) -> Vec<Row> {
    instances
        .iter()
        .map(|instance| {
            let name = instance.name.clone();
            let (state, status) = match busy(&name) {
                Some(op) => (State::Busy, op.progress_label().to_owned()),
                None if failed(&name) => (State::Failed, format!("{} · error", instance.status)),
                None if instance.is_running() => (State::Running, instance.status.clone()),
                None => (State::Stopped, instance.status.clone()),
            };
            let title = format!("{name} — {status}");
            Row { name, state, title }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Start(String),
    Stop(String),
    Restart(String),
    /// Bring up the window, selecting the profile if one is given.
    Show(Option<String>),
    /// Bring up the window on the Settings page.
    Settings,
    /// Bring up the window on the new-profile form.
    NewProfile,
    Quit,
}

impl Command {
    pub fn id(&self) -> String {
        match self {
            Command::Start(name) => format!("start:{name}"),
            Command::Stop(name) => format!("stop:{name}"),
            Command::Restart(name) => format!("restart:{name}"),
            Command::Show(Some(name)) => format!("show:{name}"),
            Command::Show(None) => "show".into(),
            Command::Settings => "settings".into(),
            Command::NewProfile => "new-profile".into(),
            Command::Quit => "quit".into(),
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        let (verb, name) = match id.split_once(':') {
            Some((verb, name)) => (verb, Some(name.to_owned())),
            None => (id, None),
        };
        Some(match (verb, name) {
            ("start", Some(name)) => Command::Start(name),
            ("stop", Some(name)) => Command::Stop(name),
            ("restart", Some(name)) => Command::Restart(name),
            ("show", name) => Command::Show(name),
            ("settings", None) => Command::Settings,
            ("new-profile", None) => Command::NewProfile,
            ("quit", None) => Command::Quit,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(name: &str, status: &str) -> Instance {
        serde_json::from_str(&format!(r#"{{"name":"{name}","status":"{status}"}}"#)).unwrap()
    }

    #[test]
    fn rows_show_status_progress_and_errors() {
        let instances = [
            instance("default", "Running"),
            instance("gpui-test", "Stopped"),
            instance("work", "Stopped"),
            instance("broken", "Broken"),
        ];
        let rows = rows(
            &instances,
            |name| (name == "work").then_some(Op::Start),
            |name| name == "broken",
        );
        let summary: Vec<_> = rows.iter().map(|r| (r.state, r.title.as_str())).collect();
        assert_eq!(
            summary,
            [
                (State::Running, "default — Running"),
                (State::Stopped, "gpui-test — Stopped"),
                (State::Busy, "work — Starting…"),
                (State::Failed, "broken — Broken · error"),
            ]
        );
    }

    #[test]
    fn command_ids_round_trip() {
        for command in [
            Command::Start("gpui-test".into()),
            Command::Stop("default".into()),
            Command::Restart("a:b".into()),
            Command::Show(Some("default".into())),
            Command::Show(None),
            Command::Settings,
            Command::NewProfile,
            Command::Quit,
        ] {
            assert_eq!(Command::parse(&command.id()), Some(command));
        }
        assert_eq!(Command::parse("start"), None);
        assert_eq!(Command::parse("quit:now"), None);
        assert_eq!(Command::parse("unknown:x"), None);
    }
}
