//! Colima operations. Each one blocks, so call it from `cx.background_executor()`.

use super::cli::{self, CliError};
use super::model::{Instance, Status};

const COLIMA: &str = "colima";

/// Filesystem usage plus allocated host blocks (not the sparse file's logical size).
pub fn vm_disk(profile: &str) -> Result<crate::docker::system::VmDisk, CliError> {
    use crate::docker::system::{parse_vm_df, valid_profile};
    use std::os::unix::fs::MetadataExt;
    if !valid_profile(profile) {
        return Err(CliError::Parse("invalid Colima profile name".into()));
    }
    let measure = |path| {
        let output = cli::run(COLIMA, &["ssh", "-p", profile, "--", "df", "-B1", path])?;
        parse_vm_df(&String::from_utf8_lossy(&output.stdout))
    };
    // Containerd profiles have no Docker data directory. Never use the OS root disk.
    let mut disk = measure("/var/lib/docker").or_else(|_| measure("/var/lib/containerd"))?;
    let instance = if profile == "default" {
        "colima".to_owned()
    } else {
        format!("colima-{profile}")
    };
    disk.allocated = std::env::var_os("HOME").and_then(|home| {
        std::fs::metadata(
            std::path::PathBuf::from(home)
                .join(".colima/_lima/_disks")
                .join(instance)
                .join("datadisk"),
        )
        .ok()
        .and_then(|metadata| metadata.blocks().checked_mul(512))
    });
    Ok(disk)
}

/// Discards unused VM blocks so the host can release sparse disk allocation.
pub fn trim(profile: &str) -> Result<(), CliError> {
    if !crate::docker::system::valid_profile(profile) {
        return Err(CliError::Parse("invalid Colima profile name".into()));
    }
    cli::run(
        COLIMA,
        &["ssh", "-p", profile, "--", "sudo", "fstrim", "-av"],
    )
    .map(drop)
}

pub fn list() -> Result<Vec<Instance>, CliError> {
    parse_list(&String::from_utf8_lossy(
        &cli::run(COLIMA, &["list", "--json"])?.stdout,
    ))
}

pub fn status(profile: &str) -> Result<Status, CliError> {
    parse_status(&String::from_utf8_lossy(
        &cli::run(COLIMA, &["status", "--json", "--profile", profile])?.stdout,
    ))
}

pub fn start(profile: &str) -> Result<(), CliError> {
    cli::run(COLIMA, &["start", "--profile", profile]).map(drop)
}

pub fn stop(profile: &str) -> Result<(), CliError> {
    stop_with_force(profile, false)
}

pub fn parse_list(stdout: &str) -> Result<Vec<Instance>, CliError> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|e| CliError::Parse(e.to_string())))
        .collect()
}

pub fn parse_status(stdout: &str) -> Result<Status, CliError> {
    serde_json::from_str(stdout.trim()).map_err(|e| CliError::Parse(e.to_string()))
}

/// Compatibility wrapper `stop(profile)` uses the normal graceful stop.
pub fn stop_with_force(profile: &str, force: bool) -> Result<(), CliError> {
    let mut args = vec!["stop", "--profile", profile];
    if force {
        args.push("--force");
    }
    cli::run(COLIMA, &args).map(drop)
}

pub fn restart(profile: &str) -> Result<(), CliError> {
    cli::run(COLIMA, &["restart", "--profile", profile]).map(drop)
}

pub fn delete(profile: &str, data: bool) -> Result<(), CliError> {
    let mut args = vec!["delete", "--profile", profile, "--force"];
    if data {
        args.push("--data");
    }
    cli::run(COLIMA, &args).map(drop)
}

pub fn version() -> Result<super::model::ColimaVersion, CliError> {
    parse_version(&String::from_utf8_lossy(
        &cli::run(COLIMA, &["version"])?.stdout,
    ))
}

/// Parses `colima version`: `colima version X` and `git commit: Y` lines, followed by
/// runtime details that are ignored.
pub fn parse_version(stdout: &str) -> Result<super::model::ColimaVersion, CliError> {
    let field = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.trim().strip_prefix(prefix))
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let version = field("colima version")
        .ok_or_else(|| CliError::Parse(format!("no version in {:?}", stdout.trim())))?;
    Ok(super::model::ColimaVersion {
        version,
        commit: field("git commit:"),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLine {
    Stdout(String),
    Stderr(String),
    Exit(i32),
}

/// Streams `colima start`. With `opts: None` the profile starts with its saved config.
pub fn start_streaming(
    profile: &str,
    opts: Option<&super::model::StartOptions>,
) -> std::sync::mpsc::Receiver<LogLine> {
    let mut command = cli::command(COLIMA);
    command.args(["start", "--profile", profile]);
    if let Some(opts) = opts {
        command.args(opts.to_args());
    }
    stream_command(command)
}

fn stream_command(mut command: std::process::Command) -> std::sync::mpsc::Receiver<LogLine> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut child = match command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                let _ = tx.send(LogLine::Stderr(error.to_string()));
                let _ = tx.send(LogLine::Exit(-1));
                return;
            }
        };
        let readers: Vec<_> = [
            (
                child
                    .stdout
                    .take()
                    .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
                false,
            ),
            (
                child
                    .stderr
                    .take()
                    .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
                true,
            ),
        ]
        .into_iter()
        .map(|(pipe, stderr)| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                if let Some(pipe) = pipe {
                    for line in BufReader::new(pipe).lines() {
                        match line {
                            Ok(line) => {
                                let _ = tx.send(if stderr {
                                    LogLine::Stderr(line)
                                } else {
                                    LogLine::Stdout(line)
                                });
                            }
                            Err(error) => {
                                let _ = tx.send(LogLine::Stderr(error.to_string()));
                                break;
                            }
                        }
                    }
                }
            })
        })
        .collect();
        let result = child.wait();
        for reader in readers {
            let _ = reader.join();
        }
        let code = match result {
            Ok(status) => status.code().unwrap_or(-1),
            Err(error) => {
                let _ = tx.send(LogLine::Stderr(error.to_string()));
                -1
            }
        };
        let _ = tx.send(LogLine::Exit(code));
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streams_both_pipes_before_exit() {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "printf 'hello\\n'; printf 'problem\\n' >&2; exit 7"]);
        let lines: Vec<_> = stream_command(command).iter().collect();
        assert!(lines.contains(&LogLine::Stdout("hello".into())));
        assert!(lines.contains(&LogLine::Stderr("problem".into())));
        assert_eq!(lines.last(), Some(&LogLine::Exit(7)));
    }
    #[test]
    fn streaming_spawn_failure_exits() {
        let lines: Vec<_> = stream_command(std::process::Command::new("/nonexistent/colima"))
            .iter()
            .collect();
        assert!(matches!(lines.first(), Some(LogLine::Stderr(_))));
        assert_eq!(lines.last(), Some(&LogLine::Exit(-1)));
    }
}
