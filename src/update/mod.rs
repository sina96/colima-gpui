//! GitHub release checking and macOS installation. No UI dependencies.
use crate::colima::cli;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;

pub const REPO: Option<&str> = option_env!("COLIMA_GPUI_UPDATE_REPO");
pub const DISABLED: &str = "Updates are available in release builds from GitHub Releases";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);
impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let parts: Vec<_> = text.strip_prefix('v').unwrap_or(text).split('.').collect();
        if parts.len() != 3
            || parts.iter().any(|p| {
                p.is_empty()
                    || !p.bytes().all(|c| c.is_ascii_digit())
                    || (p.len() > 1 && p.starts_with('0'))
            })
        {
            return None;
        }
        Some(Self(
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        ))
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub html_url: String,
    pub assets: Vec<Asset>,
}
impl Release {
    pub fn version(&self) -> Option<Version> {
        Version::parse(&self.tag_name)
    }
    pub fn version_text(&self) -> &str {
        self.tag_name.strip_prefix('v').unwrap_or(&self.tag_name)
    }
    pub fn asset(&self, arch: &str) -> Option<&Asset> {
        let arch = match arch {
            "aarch64" => "arm64",
            "x86_64" => "x86_64",
            _ => return None,
        };
        let name = format!("Colima-GPUI-{}-macos-{arch}.zip", self.version_text());
        self.assets.iter().find(|a| a.name == name)
    }
}
pub fn running_bundle() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let bundle = exe.parent()?.parent()?.parent()?;
    (bundle.extension()? == "app"
        && exe.parent()?.file_name()? == "MacOS"
        && exe.parent()?.parent()?.file_name()? == "Contents")
        .then(|| bundle.to_owned())
}
pub fn enabled() -> bool {
    REPO.is_some_and(valid_repo) && running_bundle().is_some()
}
fn valid_repo(repo: &str) -> bool {
    let parts: Vec<_> = repo.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
}
/// `COLIMA_GPUI_UPDATE_API` replaces the GitHub API for testing against a local server. It
/// works only in debug builds and in builds compiled with `COLIMA_GPUI_UPDATE_TEST` set (for
/// the bundled end-to-end test); release builds from the workflow ignore it.
fn api_override() -> Option<String> {
    if cfg!(debug_assertions) || option_env!("COLIMA_GPUI_UPDATE_TEST").is_some() {
        std::env::var("COLIMA_GPUI_UPDATE_API").ok()
    } else {
        None
    }
}
fn curl(url: &str, output: Option<&Path>) -> Result<Vec<u8>, String> {
    let mock = api_override();
    if !url.starts_with("https://")
        && !mock.as_ref().is_some_and(|base| {
            (base.starts_with("http://127.0.0.1:") || base.starts_with("http://localhost:"))
                && url.starts_with(&format!("{}/", base.trim_end_matches('/')))
        })
    {
        return Err("Update URL must use HTTPS".into());
    }
    let mut cmd = cli::command("/usr/bin/curl");
    cmd.args([
        "-fsSL",
        "--max-time",
        if output.is_some() { "300" } else { "20" },
        "--proto",
        if mock.is_some() {
            "=http,https"
        } else {
            "=https"
        },
        "--proto-redir",
        if mock.is_some() {
            "=http,https"
        } else {
            "=https"
        },
        "-H",
        "Accept: application/vnd.github+json",
    ]);
    if let Some(path) = output {
        cmd.arg("--output").arg(path);
    }
    let result = cmd.arg("--").arg(url).output().map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "Download failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    Ok(result.stdout)
}
pub fn check() -> Result<Option<Release>, String> {
    if !enabled() {
        return Err(DISABLED.into());
    }
    let base = api_override().unwrap_or_else(|| "https://api.github.com".into());
    let data = curl(
        &format!(
            "{}/repos/{}/releases/latest",
            base.trim_end_matches('/'),
            REPO.unwrap()
        ),
        None,
    )?;
    let release: Release =
        serde_json::from_slice(&data).map_err(|e| format!("Invalid release: {e}"))?;
    if release
        .version()
        .is_some_and(|v| v > Version::parse(env!("CARGO_PKG_VERSION")).unwrap())
    {
        Ok(Some(release))
    } else {
        Ok(None)
    }
}
pub fn checksum<'a>(sums: &'a str, name: &str) -> Result<&'a str, String> {
    let mut found = None;
    for line in sums.lines() {
        if line.len() < 66 {
            continue;
        }
        let (Some(hash), Some(rest)) = (line.get(..64), line.get(64..)) else {
            continue;
        };
        if rest.strip_prefix("  ").or_else(|| rest.strip_prefix(" *")) == Some(name) {
            if found.is_some() || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err("Invalid or duplicate checksum".into());
            }
            found = Some(hash);
        }
    }
    found.ok_or_else(|| "Release checksum is missing".into())
}
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn private_temp() -> Result<Temp, String> {
    // An explicit template with X's works with both BSD and GNU mktemp.
    let template = std::env::temp_dir().join("colima-gpui-update.XXXXXXXX");
    let output = cli::run("/usr/bin/mktemp", &["-d", &template.to_string_lossy()])
        .map_err(|e| e.to_string())?;
    Ok(Temp(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    )))
}
fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<Vec<u8>, String> {
    let output = cli::command(program)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "{program}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
fn plist(bundle: &Path, key: &str) -> Result<String, String> {
    let file = bundle.join("Contents/Info.plist");
    let value = run(
        "/usr/libexec/PlistBuddy",
        &[
            "-c".as_ref(),
            format!("Print :{key}").as_ref(),
            file.as_os_str(),
        ],
    )?;
    Ok(String::from_utf8_lossy(&value).trim().to_owned())
}
/// Validates everything before returning a detached replacement helper.
pub fn install(release: &Release) -> Result<(), String> {
    let bundle = running_bundle().ok_or("The running app is not in an app bundle")?;
    // macOS runs a quarantined app that wasn't moved out of the disk image or Downloads from
    // a hidden read-only copy ("App Translocation"); it can't be replaced there.
    if bundle
        .components()
        .any(|c| c.as_os_str() == "AppTranslocation")
    {
        return Err("Move Colima GPUI to the Applications folder, open it from there, then install the update.".into());
    }
    let parent = bundle.parent().ok_or("App has no parent folder")?;
    // Actual create permission, including ACLs; no attempt to elevate privileges.
    let probe = parent.join(format!(".colima-update-write-{}", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|_| {
            "The app's folder is not writable. Download the update from View on GitHub.".to_owned()
        })?;
    std::fs::remove_file(probe).map_err(|e| e.to_string())?;
    let old = bundle.with_extension("app.old");
    if old.symlink_metadata().is_ok() {
        return Err("An earlier .app.old backup exists; resolve it before updating".into());
    }
    let version = release.version().ok_or("Invalid release version")?;
    if version <= Version::parse(env!("CARGO_PKG_VERSION")).unwrap() {
        return Err("Release is not newer than this app".into());
    }
    let asset = release
        .asset(std::env::consts::ARCH)
        .ok_or("No update for this architecture")?;
    let sums = release
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS")
        .ok_or("Release has no SHA256SUMS")?;
    let temp = private_temp()?;
    let zip = temp.0.join("update.zip");
    let sums_path = temp.0.join("SHA256SUMS");
    curl(&asset.browser_download_url, Some(&zip))?;
    curl(&sums.browser_download_url, Some(&sums_path))?;
    if std::fs::metadata(&zip).map_err(|e| e.to_string())?.len() != asset.size {
        return Err("Download size differs from release asset".into());
    }
    let sums_text = std::fs::read_to_string(sums_path).map_err(|e| e.to_string())?;
    let expected = checksum(&sums_text, &asset.name)?;
    let actual = run(
        "/usr/bin/shasum",
        &["-a".as_ref(), "256".as_ref(), zip.as_os_str()],
    )?;
    if !String::from_utf8_lossy(&actual)
        .split_whitespace()
        .next()
        .is_some_and(|hash| hash.eq_ignore_ascii_case(expected))
    {
        return Err("Update checksum mismatch".into());
    }
    // Reject traversal entries before ditto touches the filesystem.
    let listing = run("/usr/bin/unzip", &["-Z1".as_ref(), zip.as_os_str()])?;
    for entry in String::from_utf8_lossy(&listing).lines() {
        if !entry.starts_with("Colima GPUI.app/") || entry.split('/').any(|p| p == "..") {
            return Err("Unexpected path in update archive".into());
        }
    }
    let details = run(
        "/usr/bin/unzip",
        &["-Z".as_ref(), "-l".as_ref(), zip.as_os_str()],
    )?;
    if String::from_utf8_lossy(&details)
        .lines()
        .any(|line| line.starts_with('l'))
    {
        return Err("Update archive contains symbolic links".into());
    }
    let unpack = temp.0.join("unpack");
    std::fs::create_dir(&unpack).map_err(|e| e.to_string())?;
    run(
        "/usr/bin/ditto",
        &[
            "-x".as_ref(),
            "-k".as_ref(),
            zip.as_os_str(),
            unpack.as_os_str(),
        ],
    )?;
    let new = unpack.join("Colima GPUI.app");
    if std::fs::read_dir(&unpack)
        .map_err(|e| e.to_string())?
        .count()
        != 1
        || std::fs::symlink_metadata(&new)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("Archive must contain one app bundle".into());
    }
    run(
        "/usr/bin/codesign",
        &["--verify".as_ref(), "--strict".as_ref(), new.as_os_str()],
    )?;
    if plist(&new, "CFBundleIdentifier")? != "dev.colima-gpui.Colima"
        || plist(&new, "CFBundleShortVersionString")? != release.version_text()
    {
        return Err("Update bundle identity or version does not match the release".into());
    }
    // Stage on the destination volume before quitting: copy and permission failures
    // can still be reported in Settings and cannot disturb the running bundle.
    let staged = parent.join(format!(".colima-update-stage-{}", std::process::id()));
    std::fs::create_dir(&staged).map_err(|e| format!("Cannot stage update: {e}"))?;
    let staged_guard = Temp(staged.clone());
    run("/usr/bin/ditto", &[new.as_os_str(), staged.as_os_str()])?;
    run(
        "/usr/bin/codesign",
        &["--verify".as_ref(), "--strict".as_ref(), staged.as_os_str()],
    )?;
    let script = temp.0.join("install.sh");
    std::fs::write(&script, include_str!("install.sh")).map_err(|e| e.to_string())?;
    let log = std::fs::File::create(temp.0.join("helper.log")).map_err(|e| e.to_string())?;
    cli::command("/bin/sh")
        .arg(&script)
        .arg(std::process::id().to_string())
        .arg(&bundle)
        .arg(&staged)
        .arg(&temp.0)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|e| e.to_string())?;
    std::mem::forget(staged_guard);
    std::mem::forget(temp); // Helper owns cleanup from this point.
    Ok(())
}
/// Called after the replacement's UI initializes, never by the old process.
pub fn acknowledge_launch() {
    let args: Vec<_> = std::env::args_os().collect();
    if let Some(ix) = args.iter().position(|a| a == "--colima-update-ready")
        && let Some(path) = args.get(ix + 1)
    {
        let path = PathBuf::from(path);
        if path.file_name().is_some_and(|n| n == "ready")
            && path.parent().is_some_and(|p| p.is_dir())
        {
            let _ = std::fs::write(path, std::process::id().to_string());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helper_paths_are_arguments_and_launch_failure_rolls_back() {
        use std::os::unix::fs::PermissionsExt;
        for fail in [false, true] {
            let temp = private_temp().unwrap();
            let app = temp
                .0
                .join("Colima ' $(touch INJECTED) `touch INJECTED` GPUI.app");
            let staged = temp.0.join("new bundle");
            let helper_temp = temp.0.join("private helper");
            std::fs::create_dir(&app).unwrap();
            std::fs::create_dir(&staged).unwrap();
            std::fs::create_dir(&helper_temp).unwrap();
            std::fs::write(app.join("version"), "old").unwrap();
            std::fs::write(staged.join("version"), "new").unwrap();
            let open = temp.0.join("fake open");
            std::fs::write(
                &open,
                r#"#!/bin/sh
if [ "$4" = "--colima-update-failed" ]; then exit 0; fi
[ "$T22_FAIL" = "false" ] || exit 1
printf '%s' "$T22_PID" > "$5"
"#,
            )
            .unwrap();
            std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o700)).unwrap();
            let script = temp.0.join("helper.sh");
            std::fs::write(
                &script,
                include_str!("install.sh").replace("/usr/bin/open", "\"$T22_OPEN\""),
            )
            .unwrap();
            let result = cli::command("/bin/sh")
                .arg(&script)
                .arg("99999999")
                .arg(&app)
                .arg(&staged)
                .arg(&helper_temp)
                .env("T22_OPEN", &open)
                .env("T22_FAIL", fail.to_string())
                .env("T22_PID", std::process::id().to_string())
                .current_dir(&temp.0)
                .output()
                .unwrap();
            assert_eq!(
                result.status.success(),
                !fail,
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                std::fs::read_to_string(app.join("version")).unwrap(),
                if fail { "old" } else { "new" }
            );
            assert!(!temp.0.join("INJECTED").exists());
            assert!(!PathBuf::from(format!("{}.old", app.display())).exists());
        }
    }

    #[test]
    fn versions() {
        assert!(Version::parse("v1.10.0") > Version::parse("1.2.9"));
        for bad in ["1.2", "1.2.3-rc1", "1.2.3.4", "01.2.3", "1.+2.3"] {
            assert_eq!(Version::parse(bad), None);
        }
    }
    #[test]
    fn release_contract() {
        let release: Release = serde_json::from_str(include_str!(
            "../../tests/fixtures/github_release_latest.json"
        ))
        .unwrap();
        assert_eq!(release.version(), Some(Version(0, 2, 0)));
        assert!(
            release
                .asset("aarch64")
                .unwrap()
                .name
                .ends_with("arm64.zip")
        );
        assert!(
            release
                .asset("x86_64")
                .unwrap()
                .name
                .ends_with("x86_64.zip")
        );
        assert!(release.asset("other").is_none());
    }
    #[test]
    fn checksums() {
        let hash = "a".repeat(64);
        assert!(checksum(&format!("{}é  app.zip", "a".repeat(63)), "app.zip").is_err());
        assert_eq!(
            checksum(&format!("{hash}  app.zip\n"), "app.zip").unwrap(),
            hash
        );
        assert!(checksum(&format!("{hash}  other.zip"), "app.zip").is_err());
        assert!(checksum(&format!("{hash}  app.zip\n{hash}  app.zip"), "app.zip").is_err());
    }
}
