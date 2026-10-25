use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Debug)]
pub enum CliError {
    /// The binary could not be started (not installed, not on `PATH`, ...).
    Spawn {
        program: String,
        source: std::io::Error,
    },
    /// The command ran and exited non-zero.
    Failed { program: String, stderr: String },
    /// The command succeeded but its output could not be parsed.
    Parse(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Spawn { program, source } => write!(f, "failed to run {program}: {source}"),
            CliError::Failed { program, stderr } => {
                write!(f, "{program} failed: {}", summarize(stderr))
            }
            CliError::Parse(msg) => write!(f, "unexpected output: {msg}"),
        }
    }
}

impl std::error::Error for CliError {}

impl CliError {
    /// The command's full error output, for a "details" view; the message itself (Display)
    /// is a short summary.
    pub fn details(&self) -> Option<&str> {
        match self {
            CliError::Failed { stderr, .. } if !stderr.trim().is_empty() => Some(stderr.trim()),
            _ => None,
        }
    }
}

/// The short message and, when there is one, the command's full error output: for an
/// [`crate::ui::ErrorNote`]'s details.
pub fn describe(error: &CliError) -> String {
    match error.details() {
        Some(details) => format!("{error}\n\n{details}"),
        None => error.to_string(),
    }
}

/// Shortens error output for display. Colima logs like
/// `time="…" level=fatal msg="exit status 1"`; this keeps each line's `msg`, and drops a bare
/// "exit status N" when another line says what went wrong.
pub fn summarize(stderr: &str) -> String {
    let lines: Vec<String> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| log_message(line).unwrap_or_else(|| line.to_owned()))
        .collect();
    let explained = lines.iter().any(|line| !line.starts_with("exit status"));
    let mut kept: Vec<&str> = Vec::new();
    for line in &lines {
        if (explained && line.starts_with("exit status")) || kept.contains(&line.as_str()) {
            continue;
        }
        kept.push(line);
    }
    match kept.as_slice() {
        [] => "no error output".into(),
        [one] => (*one).to_owned(),
        [first, second, ..] => format!("{first}; {second}"),
    }
}

/// The `msg="…"` value of a logrus line (`time=… level=… msg="…"`), unescaped.
fn log_message(line: &str) -> Option<String> {
    if !line.starts_with("time=") || !line.contains(" level=") {
        return None;
    }
    let rest = &line[line.find(" msg=")? + 5..];
    let Some(quoted) = rest.strip_prefix('"') else {
        return Some(
            rest.split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned(),
        );
    };
    let mut message = String::new();
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => message.extend(chars.next()),
            '"' => break,
            c => message.push(c),
        }
    }
    Some(message)
}

/// Search directories also become the child's PATH (Finder supplies a bare PATH).
pub fn search_dirs(home: Option<&Path>, path: Option<&OsStr>) -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(home) = home {
        dirs.push(home.join(".nix-profile/bin"));
    }
    if let Some(path) = path {
        dirs.extend(std::env::split_paths(path));
    }
    let mut unique = Vec::new();
    for dir in dirs {
        if !unique.contains(&dir) {
            unique.push(dir);
        }
    }
    unique
}

fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub fn resolve_binary(program: &str, override_path: Option<&OsStr>, dirs: &[PathBuf]) -> PathBuf {
    if let Some(path) = override_path {
        return PathBuf::from(path);
    }
    if program.contains('/') {
        return PathBuf::from(program);
    }
    dirs.iter()
        .map(|d| d.join(program))
        .find(|p| executable(p))
        .unwrap_or_else(|| program.into())
}

/// All backends, including streaming commands, use this command factory.
pub fn command(program: &str) -> Command {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let dirs = search_dirs(home.as_deref(), std::env::var_os("PATH").as_deref());
    let override_path = if program == "colima" {
        std::env::var_os("COLIMA_BIN")
    } else {
        None
    };
    // The child runs in `/` (below), so relative paths must be made absolute against our own
    // working directory first: a relative COLIMA_BIN, program or PATH entry.
    let cwd = std::env::current_dir().ok();
    let absolute = |path: PathBuf| match &cwd {
        Some(cwd) if path.is_relative() => cwd.join(path),
        _ => path,
    };
    let dirs: Vec<PathBuf> = dirs.into_iter().map(absolute).collect();
    let binary = resolve_binary(program, override_path.as_deref(), &dirs);
    // A bare name that wasn't found stays bare, for the child's PATH lookup.
    let binary = if binary.components().count() > 1 {
        absolute(binary)
    } else {
        binary
    };
    let mut command = Command::new(binary);
    let path: OsString =
        std::env::join_paths(dirs).expect("search directories contain no path separator");
    command.env("PATH", path);
    // Start every child in `/`, as a Finder-launched app does. `colima ssh` runs its command
    // in the VM from the caller's directory, and fails when that directory isn't mounted in
    // the VM (e.g. the project folder under `cargo run`).
    command.current_dir("/");
    command
}

/// Runs to completion, retaining stderr on failure.
pub fn run(program: &str, args: &[&str]) -> Result<Output, CliError> {
    let output = command(program)
        .args(args)
        .output()
        .map_err(|source| CliError::Spawn {
            program: program.into(),
            source,
        })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(CliError::Failed {
            program: program.into(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colima_errors_are_summarized() {
        // Captured from `colima ssh` run from a folder the VM doesn't mount.
        let stderr = "bash: line 1: cd: /Users/me/projects/app: No such file or directory\n\
            time=\"2026-10-25T17:08:32+01:00\" level=fatal msg=\"exit status 1\"\n";
        assert_eq!(
            summarize(stderr),
            "bash: line 1: cd: /Users/me/projects/app: No such file or directory"
        );
        assert_eq!(
            summarize("time=\"x\" level=fatal msg=\"exit status 1\""),
            "exit status 1"
        );
        assert_eq!(
            summarize("time=\"x\" level=fatal msg=\"profile \\\"gpui-test\\\" not running\"\n"),
            "profile \"gpui-test\" not running"
        );
        assert_eq!(summarize("one\ntwo\nthree\none"), "one; two");
        assert_eq!(summarize("  \n"), "no error output");
        let error = CliError::Failed {
            program: "colima".into(),
            stderr: stderr.into(),
        };
        assert!(error.to_string().starts_with("colima failed: bash: line 1"));
        assert!(error.details().unwrap().contains("level=fatal"));
        let described = describe(&error);
        assert!(described.starts_with("colima failed: bash: line 1"));
        assert!(described.contains("level=fatal msg="));
    }

    #[test]
    fn children_start_in_the_root_directory() {
        assert_eq!(
            command("colima").get_current_dir(),
            Some(std::path::Path::new("/"))
        );
        // A relative program is resolved against our directory, not the child's `/`.
        let program = PathBuf::from(command("bin/tool").get_program());
        assert!(program.is_absolute());
        assert!(program.ends_with("bin/tool"));
        // A bare name that isn't found is left to the child's PATH lookup.
        assert_eq!(
            command("no-such-tool-colima-gpui").get_program(),
            "no-such-tool-colima-gpui"
        );
    }

    #[test]
    fn resolves_fake_directory_and_override() {
        let dir = std::env::temp_dir().join(format!("colima-resolve-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("colima");
        std::fs::copy("/bin/sh", &binary).unwrap();
        assert_eq!(
            resolve_binary("colima", None, std::slice::from_ref(&dir)),
            binary
        );
        assert_eq!(
            resolve_binary(
                "colima",
                Some(OsStr::new("/override")),
                std::slice::from_ref(&dir)
            ),
            PathBuf::from("/override")
        );
        assert!(
            search_dirs(Some(Path::new("/home/test")), None)
                .contains(&PathBuf::from("/home/test/.nix-profile/bin"))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
