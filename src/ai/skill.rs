use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::colima::cli;

pub const GITHUB_URL: &str = "https://github.com/abiosoft/colima/tree/main/skills";
pub const RAW_URL: &str = "https://raw.githubusercontent.com/abiosoft/colima/main/skills";
const LIST_URL: &str = "https://api.github.com/repos/abiosoft/colima/contents/skills/references";
const KNOWN: [&str; 5] = [
    "automation.md",
    "configuration.md",
    "install.md",
    "runtimes.md",
    "troubleshooting.md",
];

/// Accept only plain Markdown file names, never paths supplied by a remote API.
pub fn parse_references(json: &str) -> Result<Vec<String>, String> {
    #[derive(serde::Deserialize)]
    struct Entry {
        name: String,
        #[serde(rename = "type")]
        kind: String,
    }
    let entries: Vec<Entry> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut names: Vec<_> = entries
        .into_iter()
        .filter(|e| e.kind == "file" && e.name.ends_with(".md"))
        .map(|e| e.name)
        .collect();
    if names.is_empty()
        || names.iter().any(|name| {
            !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                || name.starts_with('.')
        })
    {
        return Err("GitHub returned no usable reference files".into());
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    cli::run(
        "/usr/bin/curl",
        &["-fsSL", "--connect-timeout", "15", "--max-time", "60", url],
    )
    .map(|output| output.stdout)
    .map_err(|e| e.to_string())
}

pub fn download(folder: &Path, replace: bool) -> Result<PathBuf, String> {
    install(folder, replace, fetch)
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn install(
    folder: &Path,
    replace: bool,
    mut fetch: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<PathBuf, String> {
    let io = |e: std::io::Error| e.to_string();
    let destination = folder.join("colima");
    if destination.symlink_metadata().is_ok() && !replace {
        return Err("A colima skill already exists; confirm replacement first.".into());
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let staging = loop {
        let path = folder.join(format!(
            ".colima-skill-download-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => break Staging(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io(e)),
        }
    };
    let names = fetch(LIST_URL)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|json| parse_references(&json).ok())
        .unwrap_or_else(|| KNOWN.map(str::to_owned).to_vec());
    let ready = staging.0.join("ready");
    std::fs::create_dir_all(ready.join("references")).map_err(io)?;
    for file in std::iter::once("SKILL.md".to_owned())
        .chain(names.into_iter().map(|name| format!("references/{name}")))
    {
        let bytes = fetch(&format!("{RAW_URL}/{file}"))?;
        if bytes.is_empty() {
            return Err(format!("Downloaded {file} was empty"));
        }
        std::fs::write(ready.join(file), bytes).map_err(io)?;
    }
    // Keep the old directory intact until all downloads have succeeded. A failed
    // final rename restores it; the backup is inside our private staging folder.
    let backup = staging.0.join("previous");
    let exists = destination.symlink_metadata().is_ok();
    if exists {
        if !replace {
            return Err(
                "A colima skill appeared during download; confirm replacement first.".into(),
            );
        }
        std::fs::rename(&destination, &backup).map_err(io)?;
    }
    if let Err(error) = std::fs::rename(&ready, &destination) {
        if exists && let Err(restore) = std::fs::rename(&backup, &destination) {
            // Preserve the backup for recovery rather than letting Drop delete it.
            let recovery = staging.0.clone();
            std::mem::forget(staging);
            return Err(format!(
                "{error}; restore failed: {restore}. Previous skill: {}",
                recovery.join("previous").display()
            ));
        }
        return Err(io(error));
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_listing() {
        let names =
            parse_references(include_str!("../../tests/fixtures/github_skill_refs.json")).unwrap();
        assert_eq!(names, KNOWN);
        assert!(parse_references(r#"[{"type":"file","name":"../bad.md"}]"#).is_err());
        assert!(parse_references("{}").is_err());
    }

    #[test]
    #[ignore = "downloads the live GitHub skill; requires network access"]
    fn live_download() {
        let folder = std::env::temp_dir().join(format!("t49-live-skill-{}", std::process::id()));
        std::fs::create_dir(&folder).unwrap();
        let target = download(&folder, false).unwrap();
        assert!(target.join("SKILL.md").metadata().unwrap().len() > 0);
        for name in KNOWN {
            assert!(
                target
                    .join("references")
                    .join(name)
                    .metadata()
                    .unwrap()
                    .len()
                    > 0
            );
        }
        assert_eq!(
            std::fs::read_dir(target.join("references"))
                .unwrap()
                .count(),
            5
        );
        assert!(download(&folder, false).is_err());
        download(&folder, true).unwrap();
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn stages_all_files_and_preserves_existing_on_failure() {
        let folder = std::env::temp_dir().join(format!("t49-skill-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let target = install(&folder, false, |url| {
            if url == LIST_URL {
                Err("offline".into())
            } else {
                Ok(url.as_bytes().to_vec())
            }
        })
        .unwrap();
        assert!(target.join("SKILL.md").is_file());
        assert_eq!(
            std::fs::read_dir(target.join("references"))
                .unwrap()
                .count(),
            5
        );
        assert!(install(&folder, false, |_| panic!("must confirm first")).is_err());
        let original = std::fs::read(target.join("SKILL.md")).unwrap();
        assert!(
            install(&folder, true, |url| {
                if url.ends_with("install.md") {
                    Err("interrupted".into())
                } else {
                    Ok(b"replacement".to_vec())
                }
            })
            .is_err()
        );
        assert_eq!(std::fs::read(target.join("SKILL.md")).unwrap(), original);
        install(&folder, true, |_| Ok(b"replacement".to_vec())).unwrap();
        assert_eq!(
            std::fs::read(target.join("SKILL.md")).unwrap(),
            b"replacement"
        );
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
