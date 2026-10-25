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
                write!(f, "{program} failed: {}", stderr.trim())
            }
            CliError::Parse(msg) => write!(f, "unexpected output: {msg}"),
        }
    }
}

impl std::error::Error for CliError {}

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
    let binary = resolve_binary(program, override_path.as_deref(), &dirs);
    let mut command = Command::new(binary);
    let path: OsString =
        std::env::join_paths(dirs).expect("search directories contain no path separator");
    command.env("PATH", path);
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
