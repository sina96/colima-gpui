//! Docker disk accounting and non-interactive cleanup. All calls block.
use crate::colima::cli::{self, CliError};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq)]
pub struct Usage {
    pub kind: String,
    pub total: u64,
    pub size: u64,
    pub reclaimable: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VmDisk {
    pub used: u64,
    pub size: u64,
    pub allocated: Option<u64>,
}

impl VmDisk {
    pub fn returnable(&self) -> Option<u64> {
        self.allocated
            .map(|allocated| allocated.saturating_sub(self.used))
    }

    pub fn nearly_full(&self) -> bool {
        self.size > 0 && self.used as u128 * 10 >= self.size as u128 * 9
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prune {
    Containers,
    DanglingImages,
    Images,
    Volumes,
    BuildCache,
    Everything,
}

impl Prune {
    pub const ALL: [Self; 6] = [
        Self::Containers,
        Self::DanglingImages,
        Self::Images,
        Self::Volumes,
        Self::BuildCache,
        Self::Everything,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Containers => "Prune stopped containers",
            Self::DanglingImages => "Prune dangling images",
            Self::Images => "Prune all unused images",
            Self::Volumes => "Prune unused volumes",
            Self::BuildCache => "Prune build cache",
            Self::Everything => "Prune everything",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Containers => "Removes all stopped containers and their writable layers.",
            Self::DanglingImages => {
                "Removes dangling images that have no tag and are not used by a container."
            }
            Self::Images => "Removes every image not used by a container, including tagged images.",
            Self::Volumes => {
                "Permanently deletes all volumes not referenced by any container, including named volumes and their data."
            }
            Self::BuildCache => "Removes all unused build cache. Future builds may take longer.",
            Self::Everything => {
                "Removes stopped containers, all unused images, unused anonymous volumes and unused build cache. Named volumes are kept by Docker system prune."
            }
        }
    }

    pub fn args(self) -> &'static [&'static str] {
        match self {
            Self::Containers => &["container", "prune", "-f"],
            Self::DanglingImages => &["image", "prune", "-f"],
            Self::Images => &["image", "prune", "-a", "-f"],
            Self::Volumes => &["volume", "prune", "-a", "-f"],
            Self::BuildCache => &["builder", "prune", "-a", "-f"],
            Self::Everything => &["system", "prune", "-a", "--volumes", "-f"],
        }
    }

    /// Docker reports all reclaimable images together; dangling-only is an upper estimate.
    pub fn estimate(self, usage: &[Usage]) -> u64 {
        usage
            .iter()
            .filter(|row| match self {
                Self::Containers => row.kind == "Containers",
                Self::DanglingImages | Self::Images => row.kind == "Images",
                Self::Volumes => row.kind == "Local Volumes",
                Self::BuildCache => row.kind == "Build Cache",
                Self::Everything => true,
            })
            .fold(0u64, |sum, row| sum.saturating_add(row.reclaimable))
    }
}

pub fn disk_usage(socket: &str) -> Result<Vec<Usage>, CliError> {
    let output = cli::run(
        "docker",
        &["--host", socket, "system", "df", "--format", "json"],
    )?;
    parse_usage(&String::from_utf8_lossy(&output.stdout))
}

pub fn prune(socket: &str, action: Prune) -> Result<u64, CliError> {
    let mut args = vec!["--host", socket];
    args.extend_from_slice(action.args());
    let output = cli::run("docker", &args)?;
    parse_reclaimed(&String::from_utf8_lossy(&output.stdout))
}

/// Decimal Docker units and binary units, including fractional values.
pub fn parse_size(input: &str) -> Result<u64, CliError> {
    let text = input.trim();
    let split = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(text.len());
    let number = text[..split]
        .parse::<f64>()
        .map_err(|_| CliError::Parse(format!("invalid size: {input}")))?;
    let multiplier = match text[split..].trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        "pb" => 1e15,
        "kib" => 1024.,
        "mib" => 1048576.,
        "gib" => 1073741824.,
        "tib" => 1099511627776.,
        _ => return Err(CliError::Parse(format!("unknown size unit: {input}"))),
    };
    let bytes = number * multiplier;
    if !bytes.is_finite() || bytes < 0. || bytes >= u64::MAX as f64 {
        return Err(CliError::Parse(format!("size out of range: {input}")));
    }
    Ok(bytes.round() as u64)
}

pub fn format_size(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = "B";
    for next in ["kB", "MB", "GB", "TB", "PB"] {
        if value < 1000. {
            break;
        }
        value /= 1000.;
        unit = next;
    }
    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {unit}")
    }
}

pub fn parse_usage(stdout: &str) -> Result<Vec<Usage>, CliError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Row {
        #[serde(rename = "Type")]
        kind: String,
        total_count: serde_json::Value,
        size: String,
        reclaimable: String,
    }
    let rows: Vec<Row> = if stdout.trim_start().starts_with('[') {
        serde_json::from_str(stdout).map_err(|e| CliError::Parse(e.to_string()))?
    } else {
        serde_json::Deserializer::from_str(stdout)
            .into_iter::<Row>()
            .collect::<Result<_, _>>()
            .map_err(|e| CliError::Parse(e.to_string()))?
    };
    if rows.is_empty() {
        return Err(CliError::Parse("empty system df output".into()));
    }
    rows.into_iter()
        .map(|row| {
            let total = row
                .total_count
                .as_u64()
                .or_else(|| row.total_count.as_str()?.parse().ok())
                .ok_or_else(|| CliError::Parse("invalid TotalCount".into()))?;
            Ok(Usage {
                kind: row.kind,
                total,
                size: parse_size(&row.size)?,
                reclaimable: parse_size(
                    row.reclaimable
                        .split(" (")
                        .next()
                        .unwrap_or(&row.reclaimable),
                )?,
            })
        })
        .collect()
}

pub fn parse_vm_df(stdout: &str) -> Result<VmDisk, CliError> {
    let fields = stdout
        .lines()
        .skip(1)
        .find_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.len() >= 6
                && matches!(
                    fields.last(),
                    Some(&"/var/lib/docker" | &"/var/lib/containerd")
                ))
            .then_some(fields)
        })
        .ok_or_else(|| CliError::Parse("missing data filesystem in df -B1".into()))?;
    let size = fields[1]
        .parse::<u64>()
        .map_err(|e| CliError::Parse(e.to_string()))?;
    let used = fields[2]
        .parse::<u64>()
        .map_err(|e| CliError::Parse(e.to_string()))?;
    if size == 0 || used > size {
        return Err(CliError::Parse("invalid filesystem usage".into()));
    }
    Ok(VmDisk {
        used,
        size,
        allocated: None,
    })
}

pub fn parse_reclaimed(stdout: &str) -> Result<u64, CliError> {
    let value = stdout
        .lines()
        .rev()
        .find_map(|line| {
            line.trim()
                .strip_prefix("Total reclaimed space:")
                // BuildKit's builder prune reports a Total line instead.
                .or_else(|| line.trim().strip_prefix("Total:"))
        })
        .ok_or_else(|| {
            CliError::Parse("cleanup completed but no reclaimed size was reported".into())
        })?;
    parse_size(value)
}

/// Docker tab views are constructed with only a socket. Recover the profile from
/// Colima's socket path, `<colima home>/<profile>/docker.sock`; never silently fall back to
/// default.
pub fn profile_from_socket(socket: &str) -> Option<String> {
    profile_from_socket_in(socket, &crate::colima::paths::colima_home()?)
}

fn profile_from_socket_in(socket: &str, colima_home: &std::path::Path) -> Option<String> {
    let path = std::path::Path::new(socket.strip_prefix("unix://").unwrap_or(socket));
    if path.file_name()?.to_str()? != "docker.sock" {
        return None;
    }
    let parent = path.parent()?;
    if parent.parent()? != colima_home {
        return None;
    }
    let profile = parent.file_name()?.to_str()?;
    valid_profile(profile).then(|| profile.to_owned())
}

pub fn valid_profile(profile: &str) -> bool {
    !profile.is_empty()
        && profile != "."
        && profile != ".."
        && profile
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_usage() {
        let rows = parse_usage(include_str!("../../tests/fixtures/docker_system_df.json")).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].total, 7);
        assert_eq!(rows[0].size, 20_670_000_000);
        assert_eq!(rows[2].reclaimable, 647_900);
        assert_eq!(Prune::Images.estimate(&rows), 4_130_000_000);
    }
    #[test]
    fn size_formats_and_bad_data() {
        for (input, bytes) in [
            ("0B", 0),
            ("1.5GB", 1_500_000_000),
            ("2 KiB", 2048),
            ("1.25MiB", 1_310_720),
        ] {
            assert_eq!(parse_size(input).unwrap(), bytes);
        }
        for input in ["N/A", "-1B", "NaN", "1XB", "1e99B", "18446744073709551616B"] {
            assert!(parse_size(input).is_err());
        }
        assert!(parse_usage("").is_err());
        assert!(
            parse_usage(r#"{"Type":"Images","TotalCount":"x","Size":"0B","Reclaimable":"0B"}"#)
                .is_err()
        );
        assert_eq!(
            parse_usage(r#"[{"Type":"Images","TotalCount":1,"Size":"1B","Reclaimable":"0B"}]"#)
                .unwrap()[0]
                .total,
            1
        );
    }
    #[test]
    fn captured_vm_df() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/docker_system_df_vm.json"
        ))
        .unwrap();
        // The old capture is the separate OS disk and must no longer be accepted.
        assert!(parse_vm_df(fixture["stdout"].as_str().unwrap()).is_err());
        // Captured data-disk output (a full 20 GB Docker data disk).
        let captured = "Filesystem 1B-blocks Used Available Use% Mounted on\n/dev/vdb1 20955348992 20728446976 0 100% /var/lib/docker\n";
        let mut disk = parse_vm_df(captured).unwrap();
        assert_eq!((disk.used, disk.size), (20_728_446_976, 20_955_348_992));
        assert!(disk.nearly_full());
        assert!(parse_vm_df(&captured.replace("/var/lib/docker", "/var/lib/containerd")).is_ok());
        // Captured from a containerd profile (Colima 0.10): its data is bind-mounted from the
        // VM's main disk, so `df` names /dev/root but mounts it at /var/lib/containerd.
        let containerd = "Filesystem       1B-blocks       Used   Available Use% Mounted on\n/dev/root      19682557952 1112477696 18553303040   6% /var/lib/containerd\n";
        let disk_c = parse_vm_df(containerd).unwrap();
        assert_eq!((disk_c.used, disk_c.size), (1_112_477_696, 19_682_557_952));
        assert_eq!(disk.returnable(), None);
        disk.allocated = Some(disk.used + 1024);
        assert_eq!(disk.returnable(), Some(1024));
        disk.allocated = Some(0);
        assert_eq!(disk.returnable(), Some(0));
        disk.used = 89;
        disk.size = 100;
        assert!(!disk.nearly_full());
        disk.used = 90;
        assert!(disk.nearly_full());
        assert!(parse_vm_df("Filesystem 1B-blocks Used Available Use% Mounted on").is_err());
        assert!(parse_vm_df("header\nroot 2 3 0 100% /").is_err());
    }
    #[test]
    fn reclaimed_output() {
        let fixture: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/docker_system_df_reclaimed.json"
        ))
        .unwrap();
        for case in fixture {
            assert_eq!(
                parse_reclaimed(case["stdout"].as_str().unwrap()).unwrap(),
                case["bytes"].as_u64().unwrap()
            );
        }
        assert!(parse_reclaimed("Deleted Images:\nsha256:abc").is_err());
        assert!(parse_reclaimed("Total reclaimed space: unknown").is_err());
    }
    #[test]
    fn socket_profile_is_explicit() {
        let home = std::path::Path::new("/Users/me/.colima");
        let profile = |socket| profile_from_socket_in(socket, home);
        assert_eq!(
            profile("unix:///Users/me/.colima/gpui-test/docker.sock").as_deref(),
            Some("gpui-test")
        );
        assert_eq!(
            profile("/Users/me/.colima/default/docker.sock").as_deref(),
            Some("default")
        );
        assert!(profile("unix:///var/run/docker.sock").is_none());
        // A profile from another COLIMA_HOME is not this app's to clean up.
        assert!(profile("unix:///Users/me/.other/default/docker.sock").is_none());
        assert_eq!(
            profile_from_socket_in(
                "unix:///Users/me/.colima-showcase/web/docker.sock",
                std::path::Path::new("/Users/me/.colima-showcase")
            )
            .as_deref(),
            Some("web")
        );
        assert!(!valid_profile("../default"));
    }
}
