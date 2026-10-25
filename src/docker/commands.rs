//! Blocking Docker operations. Call on the background executor.
use super::model::Container;
use crate::colima::cli::{self, CliError};

/// Put the host before the subcommand, overriding the user's active context.
pub fn args(socket: &str, command: &[&str]) -> Result<Vec<String>, CliError> {
    if socket.trim().is_empty() {
        return Err(CliError::Parse("Docker socket is empty".into()));
    }
    Ok(["--host", socket]
        .into_iter()
        .chain(command.iter().copied())
        .map(str::to_owned)
        .collect())
}

fn run(socket: &str, command: &[&str]) -> Result<String, CliError> {
    let args = args(socket, command)?;
    let output = cli::run(
        "docker",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    // Docker logs writes container stderr to the process stderr even on success.
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(text)
}

pub fn parse_list(text: &str) -> Result<Vec<Container>, CliError> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|e| CliError::Parse(e.to_string())))
        .collect()
}

pub fn list(socket: &str) -> Result<Vec<Container>, CliError> {
    parse_list(&run(socket, &["ps", "-a", "--format", "json"])?)
}

pub fn start(socket: &str, id: &str) -> Result<(), CliError> {
    run(socket, &["start", "--", id]).map(drop)
}
pub fn stop(socket: &str, id: &str) -> Result<(), CliError> {
    run(socket, &["stop", "--", id]).map(drop)
}
pub fn restart(socket: &str, id: &str) -> Result<(), CliError> {
    run(socket, &["restart", "--", id]).map(drop)
}
pub fn remove(socket: &str, id: &str, force: bool) -> Result<(), CliError> {
    let mut command = vec!["rm"];
    if force {
        command.push("--force");
    }
    command.extend(["--", id]);
    run(socket, &command).map(drop)
}
pub fn logs_tail(socket: &str, id: &str, n: usize) -> Result<String, CliError> {
    run(socket, &["logs", "--tail", &n.to_string(), "--", id])
}

/// A log receiver whose lifetime also controls the child, including quiet streams.
/// Split it when handing the channel to LogPanel, keeping the guard in its owner.
pub struct Receiver<T> {
    receiver: std::sync::mpsc::Receiver<T>,
    guard: LogGuard,
}

impl<T> Receiver<T> {
    pub fn into_parts(self) -> (std::sync::mpsc::Receiver<T>, LogGuard) {
        (self.receiver, self.guard)
    }
}

impl<T> std::ops::Deref for Receiver<T> {
    type Target = std::sync::mpsc::Receiver<T>;
    fn deref(&self) -> &Self::Target {
        &self.receiver
    }
}

/// Dropping this guard cancels and reaps the Docker CLI child.
pub struct LogGuard(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for LogGuard {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Release);
    }
}

/// Follow both output pipes without blocking the caller. The child starts on a worker.
pub fn logs_follow(
    socket: &str,
    id: &str,
    tail: usize,
) -> Receiver<crate::colima::commands::LogLine> {
    let command = args(
        socket,
        &["logs", "--follow", "--tail", &tail.to_string(), "--", id],
    )
    .map(|args| {
        let mut command = cli::command("docker");
        command.args(args);
        command
    });
    stream_logs(command)
}

fn stream_logs(
    command: Result<std::process::Command, CliError>,
) -> Receiver<crate::colima::commands::LogLine> {
    use crate::colima::commands::LogLine;
    use std::{
        io::{BufRead, BufReader},
        process::Stdio,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let guard = LogGuard(cancelled.clone());
    std::thread::spawn(move || {
        let child = command.and_then(|mut command| {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|source| CliError::Spawn {
                    program: "docker logs".into(),
                    source,
                })
        });
        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                let _ = tx.send(LogLine::Stderr(error.to_string()));
                let _ = tx.send(LogLine::Exit(-1));
                return;
            }
        };
        let readers: Vec<_> = [
            (
                Box::new(child.stdout.take().expect("piped stdout"))
                    as Box<dyn std::io::Read + Send>,
                false,
            ),
            (
                Box::new(child.stderr.take().expect("piped stderr"))
                    as Box<dyn std::io::Read + Send>,
                true,
            ),
        ]
        .into_iter()
        .map(|(pipe, stderr)| {
            let tx = tx.clone();
            let cancelled = cancelled.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(pipe).lines() {
                    let line = match line {
                        Ok(line) if stderr => LogLine::Stderr(line),
                        Ok(line) => LogLine::Stdout(line),
                        Err(error) => {
                            let _ = tx.send(LogLine::Stderr(error.to_string()));
                            break;
                        }
                    };
                    if tx.send(line).is_err() {
                        cancelled.store(true, Ordering::Release);
                        break;
                    }
                }
            })
        })
        .collect();
        let status = loop {
            if cancelled.load(Ordering::Acquire) {
                let _ = child.kill();
                break child.wait();
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(error);
                }
            }
        };
        for reader in readers {
            let _ = reader.join();
        }
        let code = match status {
            Ok(status) => status.code().unwrap_or(-1),
            Err(error) => {
                let _ = tx.send(LogLine::Stderr(error.to_string()));
                -1
            }
        };
        let _ = tx.send(LogLine::Exit(code));
    });
    Receiver {
        receiver: rx,
        guard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_and_invalid_output() {
        let containers = parse_list(include_str!("../../tests/fixtures/docker_ps.json")).unwrap();
        assert_eq!(containers.len(), 2);
        assert_eq!(containers[0].names, "hello");
        assert_eq!(containers[1].state, "exited");
        assert!(parse_list("\n").unwrap().is_empty());
        assert!(parse_list("oops").is_err());
    }
    #[test]
    fn streams_both_pipes_before_exit_and_reports_spawn_errors() {
        use crate::colima::commands::LogLine;
        use std::time::Duration;
        let mut command = cli::command("/bin/sh");
        command.args(["-c", "printf 'hello\\n'; printf 'problem\\n' >&2; exit 7"]);
        let stream = stream_logs(Ok(command));
        let mut lines = Vec::new();
        loop {
            let line = stream.recv_timeout(Duration::from_secs(3)).unwrap();
            let done = matches!(line, LogLine::Exit(_));
            lines.push(line);
            if done {
                break;
            }
        }
        assert!(lines.contains(&LogLine::Stdout("hello".into())));
        assert!(lines.contains(&LogLine::Stderr("problem".into())));
        assert_eq!(lines.last(), Some(&LogLine::Exit(7)));
        let stream = stream_logs(Ok(cli::command("/no/such/t36-command")));
        assert!(matches!(
            stream.recv_timeout(Duration::from_secs(3)).unwrap(),
            LogLine::Stderr(_)
        ));
        assert_eq!(
            stream.recv_timeout(Duration::from_secs(3)).unwrap(),
            LogLine::Exit(-1)
        );
    }

    #[test]
    fn guard_cancels_a_quiet_child_and_closes_both_pipes() {
        use crate::colima::commands::LogLine;
        use std::time::Duration;
        let mut command = cli::command("/bin/sh");
        command.args(["-c", "echo ready; exec /bin/sleep 60"]);
        let (receiver, guard) = stream_logs(Ok(command)).into_parts();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(3)).unwrap(),
            LogLine::Stdout("ready".into())
        );
        drop(guard);
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(3)).unwrap(),
            LogLine::Exit(-1)
        );
        assert!(receiver.recv_timeout(Duration::from_secs(3)).is_err());
    }

    #[test]
    fn dropping_receiver_reaps_a_silent_child() {
        use crate::colima::commands::LogLine;
        use std::time::{Duration, Instant};
        let mut command = cli::command("/bin/sh");
        command.args(["-c", "echo $$; exec /bin/sleep 60"]);
        let stream = stream_logs(Ok(command));
        let LogLine::Stdout(pid) = stream.recv_timeout(Duration::from_secs(3)).unwrap() else {
            panic!("missing child PID");
        };
        drop(stream);
        let deadline = Instant::now() + Duration::from_secs(3);
        while cli::run("/bin/kill", &["-0", &pid]).is_ok() {
            assert!(
                Instant::now() < deadline,
                "log child survived receiver drop"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Manual integration check; the caller creates and removes only t36-logs.
    #[test]
    #[ignore = "requires the shared gpui-test Docker socket and t36-logs container"]
    fn follows_real_t36_container() {
        use crate::colima::commands::LogLine;
        use std::time::Duration;
        let socket = std::env::var("T36_DOCKER_SOCKET").expect("explicit gpui-test socket");
        let stream = logs_follow(&socket, "t36-logs", 0);
        let mut stdout = 0;
        let mut stderr = 0;
        while stdout < 2 || stderr < 2 {
            match stream.recv_timeout(Duration::from_secs(10)).unwrap() {
                LogLine::Stdout(text) if text.starts_with("t36-out") => stdout += 1,
                LogLine::Stderr(text) if text.starts_with("t36-err") => stderr += 1,
                line => panic!("unexpected log: {line:?}"),
            }
        }
        // The guard makes a quiet receiver drop observable to the worker too.
        let (receiver, guard) = stream.into_parts();
        drop(guard);
        loop {
            if let LogLine::Exit(_) = receiver.recv_timeout(Duration::from_secs(3)).unwrap() {
                break;
            }
        }
    }

    #[test]
    fn every_command_has_explicit_host() {
        for command in [
            vec!["ps", "-a", "--format", "json"],
            vec!["start", "--", "id"],
            vec!["stop", "--", "id"],
            vec!["restart", "--", "id"],
            vec!["rm", "--", "id"],
            vec!["rm", "--force", "--", "id"],
            vec!["logs", "--tail", "200", "--", "id"],
        ] {
            let built = args("unix:///test.sock", &command).unwrap();
            assert_eq!(&built[..2], ["--host", "unix:///test.sock"]);
            assert_eq!(&built[2..], command);
        }
        assert!(args("", &["ps"]).is_err());
    }
}

/// The sections used by the container Inspect page. Docker may return null maps.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContainerInspect {
    pub id: String,
    pub config: InspectConfig,
    #[serde(default)]
    pub mounts: Vec<InspectMount>,
    pub network_settings: InspectNetworks,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InspectConfig {
    pub env: Option<Vec<String>>,
    pub cmd: Option<Vec<String>>,
    pub entrypoint: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InspectMount {
    /// Docker volume name; bind and tmpfs mounts have no name.
    pub name: Option<String>,
    #[serde(rename = "Type")]
    pub kind: String,
    pub source: String,
    pub destination: String,
    #[serde(rename = "RW")]
    pub writable: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InspectNetworks {
    pub ports: Option<std::collections::BTreeMap<String, Option<Vec<InspectPort>>>>,
    pub networks: Option<std::collections::BTreeMap<String, InspectNetwork>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InspectPort {
    #[serde(rename = "HostIp")]
    pub host_ip: String,
    pub host_port: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct InspectNetwork {
    #[serde(rename = "IPAddress", default)]
    pub address: String,
    #[serde(rename = "Gateway", default)]
    pub gateway: String,
}

pub fn parse_inspect(text: &str) -> Result<ContainerInspect, CliError> {
    let mut rows: Vec<ContainerInspect> =
        serde_json::from_str(text).map_err(|error| CliError::Parse(error.to_string()))?;
    if rows.len() != 1 {
        return Err(CliError::Parse(
            "expected exactly one inspected container".into(),
        ));
    }
    Ok(rows.remove(0))
}

pub fn inspect(socket: &str, id: &str) -> Result<ContainerInspect, CliError> {
    parse_inspect(&run(socket, &["container", "inspect", "--", id])?)
}

#[cfg(test)]
mod inspect_tests {
    use super::*;

    #[test]
    fn captured_inspect_sections_and_null_fields() {
        let row = parse_inspect(include_str!(
            "../../tests/fixtures/docker_container_inspect.json"
        ))
        .unwrap();
        assert!(!row.id.is_empty());
        assert!(
            row.config
                .env
                .unwrap()
                .iter()
                .any(|env| env == "API_TOKEN=t26-secret")
        );
        assert_eq!(row.mounts[0].name.as_deref(), Some("t26-data"));
        assert_eq!(row.mounts[0].destination, "/data");
        assert!(row.mounts[0].writable);
        assert!(row.network_settings.ports.unwrap()["8080/tcp"].is_some());
        assert!(
            row.network_settings
                .networks
                .unwrap()
                .contains_key("bridge")
        );
        let row = parse_inspect(include_str!(
            "../../tests/fixtures/docker_container_inspect_null.json"
        ))
        .unwrap();
        assert!(row.config.env.is_none());
        assert!(row.network_settings.ports.is_none());
        for invalid in ["oops", "[]", "[{}]"] {
            assert!(parse_inspect(invalid).is_err());
        }
        assert!(inspect("", "id").is_err());
    }
}
