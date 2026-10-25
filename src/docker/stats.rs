//! Blocking resource measurements. Run on the background executor.
use serde::Deserialize;

use crate::colima::cli::{self, CliError};
use crate::docker::commands;

#[derive(Debug, Deserialize)]
pub struct ContainerStats {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "CPUPerc")]
    pub cpu: String,
    #[serde(rename = "MemUsage")]
    pub memory: String,
}

impl ContainerStats {
    /// `memory` rounded for the table: "1.222GiB / 7.737GiB" becomes "1.22 GiB / 7.74 GiB".
    /// The detail page shows Docker's full value.
    pub fn memory_short(&self) -> String {
        self.memory
            .split(" / ")
            .map(round_size)
            .collect::<Vec<_>>()
            .join(" / ")
    }

    pub fn matches(&self, id: &str) -> bool {
        !self.id.is_empty()
            && !id.is_empty()
            && (id.starts_with(&self.id) || self.id.starts_with(id))
    }
}

/// "1.222GiB" becomes "1.22 GiB" (at most two decimals, trailing zeros dropped). Anything
/// that isn't a number followed by a unit is returned unchanged.
fn round_size(size: &str) -> String {
    let size = size.trim();
    let split = size
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(size.len());
    let (number, unit) = size.split_at(split);
    match number.parse::<f64>() {
        Ok(value) if !unit.is_empty() => {
            let rounded = format!("{value:.2}");
            let rounded = rounded.trim_end_matches('0').trim_end_matches('.');
            format!("{rounded} {}", unit.trim())
        }
        _ => size.to_owned(),
    }
}

pub fn parse_containers(text: &str) -> Result<Vec<ContainerStats>, CliError> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|e| CliError::Parse(e.to_string())))
        .collect()
}

pub fn containers(socket: &str) -> Result<Vec<ContainerStats>, CliError> {
    let args = commands::args(socket, &["stats", "--no-stream", "--format", "json"])?;
    let output = cli::run(
        "docker",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    parse_containers(&String::from_utf8_lossy(&output.stdout))
}

#[derive(Debug)]
pub struct VmUsage {
    pub memory_used: u64,
    pub memory_total: u64,
    /// Linux load averages over 1, 5, and 15 minutes (not CPU percentages).
    pub load: [f64; 3],
}

pub fn parse_vm(memory: &str, load: &str) -> Result<VmUsage, CliError> {
    let invalid = || CliError::Parse("expected free -b memory and /proc/loadavg".into());
    let row = memory
        .lines()
        .find_map(|line| line.trim().strip_prefix("Mem:"))
        .ok_or_else(invalid)?;
    let mut values = row.split_whitespace();
    let memory_total: u64 = values
        .next()
        .ok_or_else(invalid)?
        .parse()
        .map_err(|_| invalid())?;
    let memory_used: u64 = values
        .next()
        .ok_or_else(invalid)?
        .parse()
        .map_err(|_| invalid())?;
    if memory_total == 0 || memory_used > memory_total {
        return Err(invalid());
    }
    let mut values = load.split_whitespace();
    let mut averages = [0.0_f64; 3];
    for average in &mut averages {
        *average = values
            .next()
            .ok_or_else(invalid)?
            .parse()
            .map_err(|_| invalid())?;
        if !average.is_finite() || *average < 0. {
            return Err(invalid());
        }
    }
    Ok(VmUsage {
        memory_used,
        memory_total,
        load: averages,
    })
}

pub fn vm(profile: &str) -> Result<VmUsage, CliError> {
    let memory = cli::run("colima", &["ssh", "-p", profile, "--", "free", "-b"])?;
    let load = cli::run(
        "colima",
        &["ssh", "-p", profile, "--", "cat", "/proc/loadavg"],
    )?;
    parse_vm(
        &String::from_utf8_lossy(&memory.stdout),
        &String::from_utf8_lossy(&load.stdout),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured read-only from a real profile, 2026-10-24.
    const STATS: &str = r#"{"BlockIO":"1.37GB / 547MB","CPUPerc":"5.27%","Container":"97be52f689fe1ea82cc9a0dccfc91f44f2efddb54e75e91bb52fb71d8b9e1ae3","ID":"97be52f689fe","MemPerc":"15.36%","MemUsage":"1.189GiB / 7.737GiB","Name":"demo-grafana","NetIO":"80MB / 383kB","PIDs":"165"}
{"BlockIO":"22.1MB / 4.1kB","CPUPerc":"0.00%","Container":"da327c9c1beca6875b24fb3ae1fccfeccdd4bc6aca27504728e21cdca42502b3","ID":"da327c9c1bec","MemPerc":"0.48%","MemUsage":"38.25MiB / 7.737GiB","Name":"demo-ministack","NetIO":"1.99kB / 126B","PIDs":"4"}"#;
    const FREE: &str = "               total        used        free      shared  buff/cache   available\nMem:      8307617792   932171776  5475041280      929792  2073833472  7375446016\nSwap:              0           0           0\n";
    const LOAD: &str = "0.15 0.22 0.18 1/370 45942\n";

    #[test]
    fn captured_container_stats_preserve_docker_units_and_match_ids() {
        let rows = parse_containers(STATS).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].cpu, "5.27%");
        assert_eq!(rows[0].memory, "1.189GiB / 7.737GiB");
        assert_eq!(rows[1].memory, "38.25MiB / 7.737GiB");
        assert!(
            rows[0].matches("97be52f689fe1ea82cc9a0dccfc91f44f2efddb54e75e91bb52fb71d8b9e1ae3")
        );
        assert!(!rows[0].matches("da327c9c1bec"));
        assert!(!rows[0].matches(""));
        assert!(parse_containers("\n").unwrap().is_empty());
        assert!(parse_containers("oops").is_err());
        assert!(parse_containers(r#"{"ID":"x"}"#).is_err());
        assert!(containers("").is_err());
    }

    #[test]
    fn memory_short_rounds_to_two_decimals() {
        let rows = parse_containers(STATS).unwrap();
        assert_eq!(rows[0].memory_short(), "1.19 GiB / 7.74 GiB");
        assert_eq!(rows[1].memory_short(), "38.25 MiB / 7.74 GiB");
        assert_eq!(round_size("0B"), "0 B");
        assert_eq!(round_size("512KiB"), "512 KiB");
        assert_eq!(round_size("1.5GiB"), "1.5 GiB");
        assert_eq!(round_size("--"), "--");
    }

    #[test]
    fn captured_vm_usage_and_invalid_measurements() {
        let usage = parse_vm(FREE, LOAD).unwrap();
        assert_eq!(usage.memory_total, 8_307_617_792);
        assert_eq!(usage.memory_used, 932_171_776);
        assert_eq!(usage.load, [0.15, 0.22, 0.18]);
        for memory in ["", "Mem: 0 0", "Mem: 1 2", "Mem: x 1", "Mem: 1"] {
            assert!(parse_vm(memory, LOAD).is_err());
        }
        for load in ["", "0.1 0.2", "NaN 0 0", "inf 0 0", "-1 0 0"] {
            assert!(parse_vm(FREE, load).is_err());
        }
    }
}
