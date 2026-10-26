//! Host paths: where Colima keeps its profiles, and how to show paths. Plain Rust: no `gpui`
//! imports.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Colima's config directory, found the way colima 0.10 finds it (`config/files.go`):
/// `$COLIMA_HOME` if it exists, then `~/.colima` if it exists, then `$XDG_CONFIG_HOME/colima`,
/// then `~/.colima` on macOS or `~/.config/colima` elsewhere. Profiles live in
/// `<home>/<profile>/`.
pub fn colima_home() -> Option<PathBuf> {
    colima_home_from(
        &Env {
            colima_home: std::env::var_os("COLIMA_HOME"),
            home: std::env::var_os("HOME"),
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME"),
            macos: cfg!(target_os = "macos"),
        },
        |path| path.exists(),
    )
}

/// Lima's directory, which holds the VM disks: `$LIMA_HOME` when set, else
/// `<colima home>/_lima`.
pub fn lima_home() -> Option<PathBuf> {
    match std::env::var_os("LIMA_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir)),
        None => colima_home().map(|home| home.join("_lima")),
    }
}

struct Env {
    colima_home: Option<OsString>,
    home: Option<OsString>,
    xdg_config_home: Option<OsString>,
    macos: bool,
}

fn colima_home_from(env: &Env, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let set = |var: &Option<OsString>| var.clone().filter(|value| !value.is_empty());
    if let Some(dir) = set(&env.colima_home).map(PathBuf::from)
        && exists(&dir)
    {
        return Some(dir);
    }
    let home = PathBuf::from(set(&env.home)?);
    let dot_colima = home.join(".colima");
    if exists(&dot_colima) {
        return Some(dot_colima);
    }
    if let Some(xdg) = set(&env.xdg_config_home) {
        return Some(PathBuf::from(xdg).join("colima"));
    }
    if env.macos {
        Some(dot_colima)
    } else {
        Some(home.join(".config/colima"))
    }
}

/// Shortens the home directory to `~` for display, keeping a `unix://` scheme:
/// `unix:///Users/me/.colima/default/docker.sock` -> `unix://~/.colima/default/docker.sock`.
/// Only for showing; copy and use the full path, since nothing expands `~` there.
pub fn tilde(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) => tilde_in(path, &home),
        Err(_) => path.to_owned(),
    }
}

fn tilde_in(path: &str, home: &str) -> String {
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return path.to_owned();
    }
    let (scheme, rest) = match path.strip_prefix("unix://") {
        Some(rest) => ("unix://", rest),
        None => ("", path),
    };
    match rest.strip_prefix(home) {
        Some(tail) if tail.is_empty() || tail.starts_with('/') => format!("{scheme}~{tail}"),
        _ => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Env, colima_home_from, tilde_in};
    use std::path::{Path, PathBuf};

    fn env(colima_home: Option<&str>, xdg: Option<&str>, macos: bool) -> Env {
        Env {
            colima_home: colima_home.map(Into::into),
            home: Some("/home/me".into()),
            xdg_config_home: xdg.map(Into::into),
            macos,
        }
    }

    fn home(env: &Env, existing: &[&str]) -> PathBuf {
        colima_home_from(env, |path| existing.iter().any(|e| Path::new(e) == path)).unwrap()
    }

    #[test]
    fn colima_home_follows_colimas_order() {
        let showcase = env(Some("/tmp/showcase"), Some("/xdg"), false);
        assert_eq!(
            home(&showcase, &["/tmp/showcase"]),
            PathBuf::from("/tmp/showcase")
        );
        // A missing COLIMA_HOME is ignored, as colima ignores it.
        assert_eq!(
            home(&showcase, &["/home/me/.colima"]),
            PathBuf::from("/home/me/.colima")
        );
        // ~/.colima wins over XDG when it exists.
        let xdg = env(None, Some("/xdg"), false);
        assert_eq!(
            home(&xdg, &["/home/me/.colima"]),
            PathBuf::from("/home/me/.colima")
        );
        assert_eq!(home(&xdg, &[]), PathBuf::from("/xdg/colima"));
        assert_eq!(
            home(&env(None, None, false), &[]),
            PathBuf::from("/home/me/.config/colima")
        );
        assert_eq!(
            home(&env(None, None, true), &[]),
            PathBuf::from("/home/me/.colima")
        );
        assert_eq!(
            home(&env(Some(""), Some(""), true), &[]),
            PathBuf::from("/home/me/.colima")
        );
    }

    #[test]
    fn shortens_home_and_keeps_scheme() {
        assert_eq!(
            tilde_in("unix:///Users/me/.colima/ocb/docker.sock", "/Users/me"),
            "unix://~/.colima/ocb/docker.sock"
        );
        assert_eq!(
            tilde_in("/home/me/.colima/default/docker.sock", "/home/me/"),
            "~/.colima/default/docker.sock"
        );
        assert_eq!(tilde_in("/Users/me", "/Users/me"), "~");
    }

    #[test]
    fn leaves_other_paths_alone() {
        assert_eq!(
            tilde_in("/Users/mel/.colima/x/docker.sock", "/Users/me"),
            "/Users/mel/.colima/x/docker.sock"
        );
        assert_eq!(
            tilde_in("unix:///var/run/docker.sock", "/Users/me"),
            "unix:///var/run/docker.sock"
        );
        assert_eq!(tilde_in("/anything", ""), "/anything");
    }
}
