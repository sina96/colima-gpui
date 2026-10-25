//! Volume metadata, usage and actions for a single Docker socket. No GPUI dependency.
use serde::Deserialize;

use crate::colima::cli::{self, CliError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    pub name: String,
    pub driver: String,
    pub size_bytes: Option<u64>,
    pub links: Option<u64>,
    pub containers: Vec<VolumeContainer>,
}

impl Volume {
    pub fn unused(&self) -> bool {
        self.links == Some(0) && self.containers.is_empty()
    }

    pub fn display_name(&self) -> String {
        if self.name.len() == 64 && self.name.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            format!("{}…", &self.name[..12])
        } else {
            self.name.clone()
        }
    }

    pub fn usage_description(&self) -> String {
        if !self.containers.is_empty() {
            self.containers
                .iter()
                .map(|container| format!("{} ({})", container.name, container.state))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            match self.links {
                Some(0) => "Unused".into(),
                Some(links) => format!("{links} container reference(s); refresh for names"),
                None => "Usage unavailable; refresh before removing".into(),
            }
        }
    }
}

/// Metadata from `volume inspect`; fetched only while its detail page is visible.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct VolumeDetails {
    #[serde(rename = "Mountpoint")]
    pub mountpoint: String,
    /// RFC 3339 creation time; empty if Docker didn't report one.
    #[serde(rename = "CreatedAt", default)]
    pub created_at: String,
    #[serde(rename = "Labels", default)]
    pub labels: Option<std::collections::BTreeMap<String, String>>,
}

fn parse_details(text: &str) -> Result<VolumeDetails, CliError> {
    let mut details: Vec<VolumeDetails> =
        serde_json::from_str(text).map_err(|error| CliError::Parse(error.to_string()))?;
    if details.len() != 1 {
        return Err(CliError::Parse(
            "expected exactly one inspected volume".into(),
        ));
    }
    Ok(details.remove(0))
}

pub fn inspect(socket: &str, name: &str) -> Result<VolumeDetails, CliError> {
    parse_details(&output(socket, &["volume", "inspect", "--", name])?)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct VolumeContainer {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Names")]
    pub name: String,
    #[serde(rename = "State")]
    pub state: String,
}

#[derive(Deserialize)]
struct ListedVolume {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Driver")]
    driver: String,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(rename = "Volumes")]
    volumes: Option<Vec<VolumeUsage>>,
}

#[derive(Deserialize)]
struct VolumeUsage {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Size")]
    size: String,
    #[serde(rename = "Links")]
    links: String,
}

fn json_lines<T: serde::de::DeserializeOwned>(text: &str) -> Result<Vec<T>, CliError> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|error| CliError::Parse(error.to_string())))
        .collect()
}

fn parse_volumes(list: &str, usage: &str) -> Result<Vec<Volume>, CliError> {
    let listed: Vec<ListedVolume> = json_lines(list)?;
    let usage: Usage =
        serde_json::from_str(usage).map_err(|error| CliError::Parse(error.to_string()))?;
    let usage = usage.volumes.unwrap_or_default();
    let mut volumes = Vec::with_capacity(listed.len());
    for volume in listed {
        let measured = usage.iter().find(|entry| entry.name == volume.name);
        volumes.push(Volume {
            name: volume.name,
            driver: volume.driver,
            size_bytes: measured
                .map(|entry| parse_size(&entry.size))
                .transpose()?
                .flatten(),
            links: measured
                .map(|entry| {
                    entry.links.parse().map_err(|_| {
                        CliError::Parse(format!("invalid volume link count: {}", entry.links))
                    })
                })
                .transpose()?,
            containers: Vec::new(),
        });
    }
    volumes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(volumes)
}

/// Docker's human-readable SI sizes are rounded, so prune totals are estimates.
fn parse_size(text: &str) -> Result<Option<u64>, CliError> {
    let text = text.trim();
    if text == "N/A" || text.is_empty() {
        return Ok(None);
    }
    let split = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(text.len());
    let number = text[..split].parse::<f64>().ok();
    let multiplier = match text[split..].trim() {
        "B" => 1.,
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "KiB" => 1024.,
        "MiB" => 1024_f64.powi(2),
        "GiB" => 1024_f64.powi(3),
        "TiB" => 1024_f64.powi(4),
        _ => return Err(CliError::Parse(format!("invalid volume size: {text}"))),
    };
    match number.map(|number| number * multiplier) {
        Some(bytes) if bytes.is_finite() && bytes >= 0. && bytes < u64::MAX as f64 => {
            Ok(Some(bytes.round() as u64))
        }
        _ => Err(CliError::Parse(format!("invalid volume size: {text}"))),
    }
}

pub fn format_size(bytes: u64) -> String {
    let mut value = bytes as f64;
    for unit in ["B", "kB", "MB", "GB", "TB"] {
        if value < 1000. || unit == "TB" {
            return if unit == "B" {
                format!("{bytes} B")
            } else {
                format!("{value:.1} {unit}")
            };
        }
        value /= 1000.;
    }
    unreachable!()
}

fn output(socket: &str, args: &[&str]) -> Result<String, CliError> {
    let mut all = vec!["--host", socket];
    all.extend_from_slice(args);
    let output = cli::run("docker", &all)?;
    String::from_utf8(output.stdout).map_err(|error| CliError::Parse(error.to_string()))
}

pub fn list(socket: &str) -> Result<Vec<Volume>, CliError> {
    let listed = output(socket, &["volume", "ls", "--format", "json"])?;
    let usage = output(socket, &["system", "df", "-v", "--format", "json"])?;
    let mut volumes = parse_volumes(&listed, &usage)?;
    for volume in &mut volumes {
        let filter = format!("volume={}", volume.name);
        volume.containers = json_lines(&output(
            socket,
            &["ps", "-a", "--filter", &filter, "--format", "json"],
        )?)?;
    }
    Ok(volumes)
}

/// Never force removal: Docker protects references added after our last refresh too.
pub fn remove(socket: &str, name: &str) -> Result<String, CliError> {
    output(socket, &["volume", "rm", "--", name])
}

pub fn prune(socket: &str) -> Result<String, CliError> {
    output(socket, &["volume", "prune", "--all", "--force"])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_volumes() -> Vec<Volume> {
        parse_volumes(
            include_str!("../../tests/fixtures/docker_volumes_list.json"),
            include_str!("../../tests/fixtures/docker_volumes_df.json"),
        )
        .unwrap()
    }

    #[test]
    fn inspect_fixture_and_missing_labels() {
        let details = parse_details(include_str!(
            "../../tests/fixtures/docker_volume_inspect.json"
        ))
        .unwrap();
        assert!(details.mountpoint.ends_with("/t28-detail/_data"));
        assert_eq!(details.created_at, "2026-10-25T00:45:55+02:00");
        assert_eq!(
            details.labels.unwrap().get("task").map(String::as_str),
            Some("t28")
        );
        for text in [
            r#"[{"Mountpoint":"/data","Labels":null}]"#,
            r#"[{"Mountpoint":"/data"}]"#,
        ] {
            assert!(parse_details(text).unwrap().labels.is_none());
        }
        for invalid in ["invalid", "[]", "[{},{}]", r#"[{"Labels":{}}]"#] {
            assert!(parse_details(invalid).is_err());
        }
    }

    #[test]
    fn real_fixtures_use_df_sizes_and_include_stopped_container_references() {
        let volumes = fixture_volumes();
        assert_eq!(volumes.len(), 2);
        assert_eq!(volumes[0].name, "t14-unused");
        assert_eq!(volumes[0].driver, "local");
        assert_eq!(volumes[0].size_bytes, Some(0));
        assert!(volumes[0].unused());
        let mut used = volumes[1].clone();
        assert_eq!(used.size_bytes, Some(65_540));
        assert_eq!(used.links, Some(1));
        assert!(!used.unused());
        used.containers = json_lines(include_str!(
            "../../tests/fixtures/docker_volumes_containers.json"
        ))
        .unwrap();
        assert_eq!(used.usage_description(), "t14-holder (created)");
        used.links = Some(0);
        assert!(!used.unused());
    }

    #[test]
    fn captured_created_container_prevents_removal_even_with_zero_links() {
        let mut volume = fixture_volumes().remove(0);
        volume.containers = json_lines(include_str!(
            "../../tests/fixtures/docker_volume_containers.json"
        ))
        .unwrap();
        assert!(!volume.unused());
        assert_eq!(volume.usage_description(), "t28-holder (created)");
    }

    #[test]
    fn empty_missing_and_invalid_measurements_are_safe() {
        assert!(
            parse_volumes("\n", r#"{"Volumes":null}"#)
                .unwrap()
                .is_empty()
        );
        let list = include_str!("../../tests/fixtures/docker_volumes_list.json");
        let missing = parse_volumes(list, r#"{"Volumes":[]}"#).unwrap();
        assert!(
            missing
                .iter()
                .all(|volume| !volume.unused() && volume.size_bytes.is_none())
        );
        assert!(parse_volumes("invalid", r#"{"Volumes":[]}"#).is_err());
        assert!(parse_volumes(list, "invalid").is_err());
        assert!(
            parse_volumes(
                list,
                r#"{"Volumes":[{"Name":"t14-used","Size":"0B","Links":"N/A"}]}"#
            )
            .is_err()
        );
    }

    #[test]
    fn sizes_and_anonymous_names() {
        for (text, bytes) in [
            ("0B", 0),
            ("1.25kB", 1250),
            ("2 MB", 2_000_000),
            ("1.5GiB", 1_610_612_736),
        ] {
            assert_eq!(parse_size(text).unwrap(), Some(bytes));
        }
        assert_eq!(parse_size("N/A").unwrap(), None);
        for invalid in ["bad", "-1B", "1XB", "1.2.3MB", "999999999999999999999TB"] {
            assert!(parse_size(invalid).is_err(), "{invalid}");
        }
        let mut volume = fixture_volumes().remove(0);
        volume.name = "a".repeat(64);
        assert_eq!(volume.display_name(), format!("{}…", "a".repeat(12)));
        volume.name = "named-volume".into();
        assert_eq!(volume.display_name(), "named-volume");
        assert_eq!(format_size(1250), "1.2 kB");
    }
}
