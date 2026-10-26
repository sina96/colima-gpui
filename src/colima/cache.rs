//! Colima's shared download cache. Blocking helpers: call off the UI thread.
use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use super::cli::{self, CliError};

/// Matches Colima 0.10.3's `config/files.go`, independently of COLIMA_HOME.
pub fn cache_home() -> Option<PathBuf> {
    cache_home_from(
        std::env::var_os("COLIMA_CACHE_HOME"),
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
        cfg!(target_os = "macos"),
    )
}

fn cache_home_from(
    colima: Option<OsString>,
    xdg: Option<OsString>,
    home: Option<OsString>,
    macos: bool,
) -> Option<PathBuf> {
    let nonempty =
        |value: Option<OsString>| value.filter(|value| !value.is_empty()).map(PathBuf::from);
    nonempty(colima)
        .or_else(|| nonempty(xdg).map(|path| path.join("colima")))
        .or_else(|| {
            nonempty(home).map(|path| {
                path.join(if macos {
                    "Library/Caches/colima"
                } else {
                    ".cache/colima"
                })
            })
        })
}

pub fn size() -> io::Result<u64> {
    directory_size(&cache_home().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Colima cache directory unavailable: HOME is unset",
        )
    })?)
}

/// Sum regular file lengths without following symlinks out of the cache or into cycles.
/// A missing cache is empty; other scan failures must not appear as a zero-byte cache.
fn directory_size(path: &Path) -> io::Result<u64> {
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut size = 0u64;
    if metadata.is_dir() {
        for entry in path.read_dir()? {
            size = size.saturating_add(directory_size(&entry?.path())?);
        }
    }
    Ok(size)
}

/// Cache pruning is host-wide; no profile or Lima asset pruning is requested.
pub fn prune() -> Result<u64, CliError> {
    let before =
        size().map_err(|error| CliError::Parse(format!("cannot measure Colima cache: {error}")))?;
    cli::run("colima", &["prune", "--force"])?;
    let after = size().map_err(|error| {
        CliError::Parse(format!(
            "cache pruned, but cannot measure remaining cache: {error}"
        ))
    })?;
    Ok(before.saturating_sub(after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_precedence_and_platform_defaults() {
        let path = |colima: &str, xdg: &str, macos| {
            cache_home_from(
                Some(colima.into()),
                Some(xdg.into()),
                Some("/home/me".into()),
                macos,
            )
            .unwrap()
        };
        assert_eq!(path("/custom", "/xdg", true), PathBuf::from("/custom"));
        assert_eq!(path("", "/xdg", true), PathBuf::from("/xdg/colima"));
        assert_eq!(
            path("", "", true),
            PathBuf::from("/home/me/Library/Caches/colima")
        );
        assert_eq!(path("", "", false), PathBuf::from("/home/me/.cache/colima"));
        assert_eq!(cache_home_from(None, None, None, false), None);
        assert_eq!(
            cache_home_from(Some("/cache".into()), None, None, false),
            Some("/cache".into())
        );
    }

    #[test]
    fn scan_counts_nested_files_and_ignores_symlinks() {
        let root = std::env::temp_dir().join(format!("t60-cache-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("asset"), [0; 17]).unwrap();
        std::fs::write(root.join("nested/asset"), [0; 23]).unwrap();
        std::os::unix::fs::symlink(&root, root.join("nested/cycle")).unwrap();
        assert_eq!(directory_size(&root).unwrap(), 40);
        assert_eq!(directory_size(&root.join("missing")).unwrap(), 0);
        assert!(directory_size(&root.join("asset/invalid")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
