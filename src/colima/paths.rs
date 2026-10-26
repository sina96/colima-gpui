//! Host paths: where Colima keeps its profiles, and how to show paths. Plain Rust: no `gpui`
//! imports.

use std::path::{Path, PathBuf};

/// Colima's config directory: `$COLIMA_HOME` when set and the folder exists, as colima itself
/// reads it, else `~/.colima`. Profiles live in `<home>/<profile>/` and Lima's VMs in
/// `<home>/_lima/`.
pub fn colima_home() -> Option<PathBuf> {
    colima_home_from(
        std::env::var_os("COLIMA_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        |dir| dir.is_dir(),
    )
}

fn colima_home_from(
    colima_home: Option<&std::ffi::OsStr>,
    home: Option<&std::ffi::OsStr>,
    is_dir: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    match colima_home.filter(|dir| !dir.is_empty() && is_dir(Path::new(dir))) {
        Some(dir) => Some(PathBuf::from(dir)),
        None => home.map(|home| Path::new(home).join(".colima")),
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
    use super::{colima_home_from, tilde_in};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn colima_home_prefers_the_env_override() {
        let home = Some(OsStr::new("/Users/me"));
        let showcase = Some(OsStr::new("/tmp/showcase"));
        assert_eq!(
            colima_home_from(showcase, home, |_| true),
            Some(PathBuf::from("/tmp/showcase"))
        );
        // Colima falls back to ~/.colima when the folder is missing; so do we.
        assert_eq!(
            colima_home_from(showcase, home, |_| false),
            Some(PathBuf::from("/Users/me/.colima"))
        );
        assert_eq!(
            colima_home_from(Some(OsStr::new("")), home, |_| true),
            Some(PathBuf::from("/Users/me/.colima"))
        );
        assert_eq!(colima_home_from(None, None, |_| true), None);
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
