//! Image metadata and actions for one Docker socket. No UI dependencies.
use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;

use crate::colima::cli::{self, CliError};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Image {
    #[serde(rename = "ID")]
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub size: String,
    pub created_at: String,
    #[serde(skip)]
    pub unique_size: Option<u64>,
    #[serde(skip)]
    pub shared_size: Option<u64>,
    #[serde(skip)]
    pub containers: Option<u64>,
}

impl Image {
    pub fn name(&self) -> String {
        format!("{}:{}", self.repository, self.tag)
    }

    pub fn dangling(&self) -> bool {
        self.repository == "<none>" && self.tag == "<none>"
    }

    /// A tag removes only that reference; dangling images must be removed by ID.
    pub fn remove_target(&self) -> String {
        if self.dangling() {
            self.id.clone()
        } else {
            self.name()
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Usage {
    #[serde(rename = "ID")]
    id: String,
    unique_size: String,
    shared_size: String,
    containers: String,
}

#[derive(Deserialize)]
struct DiskUsage {
    #[serde(rename = "Images")]
    images: Option<Vec<Usage>>,
}

fn id(value: &str) -> &str {
    value.strip_prefix("sha256:").unwrap_or(value)
}

pub fn parse(list: &str, usage: &str) -> Result<Vec<Image>, CliError> {
    let usage: DiskUsage =
        serde_json::from_str(usage).map_err(|error| CliError::Parse(error.to_string()))?;
    let usage = usage.images.unwrap_or_default();
    list.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut image: Image =
                serde_json::from_str(line).map_err(|error| CliError::Parse(error.to_string()))?;
            if let Some(row) = usage.iter().find(|row| {
                let short = id(&image.id);
                !short.is_empty() && id(&row.id).starts_with(short)
            }) {
                image.unique_size = size_bytes(&row.unique_size);
                image.shared_size = size_bytes(&row.shared_size);
                image.containers = row.containers.parse().ok();
            }
            Ok(image)
        })
        .collect()
}

/// Docker uses decimal units; accept IEC units too, without treating N/A as zero.
pub fn size_bytes(value: &str) -> Option<u64> {
    let value = value.trim();
    let split = value
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(value.len());
    let number: f64 = value[..split].parse().ok()?;
    let multiplier = match value[split..].trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        "kib" => 1024.,
        "mib" => 1024_f64.powi(2),
        "gib" => 1024_f64.powi(3),
        "tib" => 1024_f64.powi(4),
        _ => return None,
    };
    let bytes = number * multiplier;
    (bytes.is_finite() && bytes >= 0. && bytes < u64::MAX as f64).then(|| bytes.round() as u64)
}

pub fn format_size(bytes: u64) -> String {
    for (unit, scale) in [("TB", 1e12), ("GB", 1e9), ("MB", 1e6), ("kB", 1e3)] {
        if bytes as f64 >= scale {
            return format!("{:.1} {unit}", bytes as f64 / scale);
        }
    }
    format!("{bytes} B")
}

/// Unique bytes are a conservative estimate: shared layers may also be freed.
/// Missing counts or sizes make the estimate unknown rather than unsafe to prune.
pub fn reclaimable(images: &[Image]) -> Option<u64> {
    let mut seen = HashSet::new();
    let mut bytes = 0u64;
    for image in images {
        if !seen.insert(id(&image.id)) {
            continue;
        }
        if image.containers? == 0 {
            bytes = bytes.checked_add(image.unique_size?)?;
        }
    }
    Some(bytes)
}

pub fn list(socket: &str) -> Result<Vec<Image>, CliError> {
    let list = cli::run(
        "docker",
        &["--host", socket, "image", "ls", "--format", "json"],
    )?;
    let usage = cli::run(
        "docker",
        &["--host", socket, "system", "df", "-v", "--format", "json"],
    )?;
    parse(
        &String::from_utf8_lossy(&list.stdout),
        &String::from_utf8_lossy(&usage.stdout),
    )
}

/// One history entry, including metadata-only (zero byte) layers.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HistoryLayer {
    #[serde(rename = "ID")]
    pub id: String,
    pub created_at: String,
    #[serde(default)]
    pub created_since: String,
    pub created_by: String,
    pub size: String,
    pub comment: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImageContainer {
    #[serde(rename = "ID")]
    pub id: String,
    pub names: String,
    pub state: String,
    pub status: String,
}

/// Metadata from `docker image inspect`; null configuration fields occur on imported images.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImageInspect {
    pub id: String,
    pub repo_tags: Option<Vec<String>>,
    #[serde(default)]
    pub created: String,
    pub size: u64,
    pub os: String,
    pub architecture: String,
    #[serde(default)]
    pub variant: String,
    pub config: Option<ImageConfig>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImageConfig {
    pub entrypoint: Option<Vec<String>>,
    pub cmd: Option<Vec<String>>,
    pub exposed_ports: Option<BTreeMap<String, serde_json::Value>>,
    pub env: Option<Vec<String>>,
    pub labels: Option<BTreeMap<String, String>>,
}

pub fn parse_inspect(output: &str) -> Result<ImageInspect, CliError> {
    let mut images: Vec<ImageInspect> =
        serde_json::from_str(output).map_err(|error| CliError::Parse(error.to_string()))?;
    if images.len() != 1 {
        return Err(CliError::Parse(
            "expected exactly one inspected image".into(),
        ));
    }
    Ok(images.remove(0))
}

pub struct ImageDetails {
    pub inspect: ImageInspect,
    pub containers: Vec<ImageContainer>,
    pub history: Vec<HistoryLayer>,
}

fn parse_lines<T: serde::de::DeserializeOwned>(output: &str) -> Result<Vec<T>, CliError> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|error| CliError::Parse(error.to_string())))
        .collect()
}

pub fn parse_history(output: &str) -> Result<Vec<HistoryLayer>, CliError> {
    parse_lines(output)
}

pub fn details(socket: &str, image_id: &str) -> Result<ImageDetails, CliError> {
    let inspect = cli::run("docker", &["--host", socket, "image", "inspect", image_id])?;
    let inspect = parse_inspect(&String::from_utf8_lossy(&inspect.stdout))?;
    let containers = cli::run(
        "docker",
        &[
            "--host",
            socket,
            "ps",
            "-a",
            "--no-trunc",
            "--filter",
            &format!("ancestor={image_id}"),
            "--format",
            "json",
        ],
    )?;
    let history = cli::run(
        "docker",
        &[
            "--host",
            socket,
            "image",
            "history",
            "--no-trunc",
            "--format",
            "json",
            image_id,
        ],
    )?;
    Ok(ImageDetails {
        inspect,
        containers: parse_lines(&String::from_utf8_lossy(&containers.stdout))?,
        history: parse_history(&String::from_utf8_lossy(&history.stdout))?,
    })
}

pub fn remove(socket: &str, target: &str, force: bool) -> Result<(), CliError> {
    let mut args = vec!["--host", socket, "image", "rm"];
    if force {
        args.push("--force");
    }
    args.push(target);
    cli::run("docker", &args).map(|_| ())
}

pub fn prune(socket: &str) -> Result<(), CliError> {
    cli::run(
        "docker",
        &["--host", socket, "image", "prune", "-a", "--force"],
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_inspect_preserves_config_and_exact_metadata() {
        let image = parse_inspect(include_str!(
            "../../tests/fixtures/docker_image_inspect.json"
        ))
        .unwrap();
        assert!(image.id.starts_with("sha256:"));
        assert_eq!(image.repo_tags.unwrap(), ["t47-image-tabs:latest"]);
        assert_eq!(image.created, "2026-10-25T11:07:25.61299694+01:00");
        assert_eq!(image.size, 3430);
        assert_eq!(
            (image.os.as_str(), image.architecture.as_str()),
            ("linux", "arm64")
        );
        let config = image.config.unwrap();
        assert_eq!(config.entrypoint.unwrap(), ["/t47-first"]);
        assert_eq!(config.cmd.unwrap(), ["--t47-check", "hello world"]);
        assert_eq!(
            config
                .exposed_ports
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            ["8080/tcp", "9090/udp"]
        );
        assert_eq!(config.env.unwrap()[2], "T47_EMPTY=");
        assert_eq!(
            config.labels.unwrap()["t47.purpose"],
            "Image detail verification"
        );
    }

    #[test]
    fn inspect_accepts_null_and_missing_optional_config_but_rejects_invalid_images() {
        let mut fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/docker_image_inspect.json"
        ))
        .unwrap();
        fixture[0]["RepoTags"] = serde_json::Value::Null;
        fixture[0]["Config"] = serde_json::json!({
            "Env": null, "Labels": null, "Cmd": null, "Entrypoint": null, "ExposedPorts": null
        });
        let image = parse_inspect(&fixture.to_string()).unwrap();
        assert!(image.repo_tags.is_none());
        let config = image.config.unwrap();
        assert!(config.env.is_none());
        assert!(config.labels.is_none());
        assert!(config.cmd.is_none());
        assert!(config.entrypoint.is_none());
        assert!(config.exposed_ports.is_none());
        for value in [serde_json::Value::Null, serde_json::json!({})] {
            fixture[0]["Config"] = value;
            let image = parse_inspect(&fixture.to_string()).unwrap();
            assert!(image.config.unwrap_or_default().env.is_none());
        }
        fixture[0].as_object_mut().unwrap().remove("Config");
        assert!(
            parse_inspect(&fixture.to_string())
                .unwrap()
                .config
                .is_none()
        );
        for invalid in ["[]", "[{},{}]", "{}", "broken", "[{}]"] {
            assert!(parse_inspect(invalid).is_err());
        }
    }

    #[test]
    fn captured_history_preserves_order_commands_and_metadata_layers() {
        let history = parse_history(include_str!(
            "../../tests/fixtures/docker_image_history.json"
        ))
        .unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].created_by, "CMD [\"/t27-data\"]");
        assert_eq!(size_bytes(&history[0].size), Some(0));
        assert_eq!(history[1].id, "<missing>");
        assert_eq!(history[2].created_by, "COPY t27-data /t27-data # buildkit");
        assert_eq!(size_bytes(&history[2].size), Some(127_000));
        assert_eq!(history[2].comment, "buildkit.dockerfile.v0");
        assert!(!history[2].created_at.is_empty());
        assert!(parse_history(" \n\n").unwrap().is_empty());
        assert!(parse_history("not json").is_err());
        assert!(parse_history(r#"{"ID":"incomplete"}"#).is_err());
    }

    #[test]
    fn captured_ancestor_containers_include_stopped_containers() {
        let containers: Vec<ImageContainer> = parse_lines(include_str!(
            "../../tests/fixtures/docker_image_containers.json"
        ))
        .unwrap();
        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].names, "t27-detail-container");
        assert_eq!(containers[0].state, "created");
        assert_eq!(containers[0].status, "Created");
        assert_eq!(containers[0].id.len(), 64);
        assert!(parse_lines::<ImageContainer>("").unwrap().is_empty());
        assert!(parse_lines::<ImageContainer>("broken").is_err());
    }

    #[test]
    fn real_default_fixtures_merge_full_ids_and_duplicate_tags() {
        let images = parse(
            include_str!("../../tests/fixtures/docker_images.json"),
            include_str!("../../tests/fixtures/docker_images_df.json"),
        )
        .unwrap();
        assert_eq!(images.len(), 8);
        assert_eq!(images[0].containers, Some(1));
        assert_eq!(images[0].unique_size, Some(263_300_000));
        assert_eq!(images[0].shared_size, Some(34));
        assert_eq!(images[0].unique_size, images[1].unique_size);
        assert_eq!(reclaimable(&images), Some(4_129_900_000));
        assert_eq!(images[0].remove_target(), "ministackorg/ministack:1.5.28");
    }

    #[test]
    fn sizes_use_decimal_or_binary_units_and_reject_unknowns() {
        for (input, expected) in [
            ("22.57kB", 22570),
            ("1.5 GB", 1_500_000_000),
            ("2MiB", 2_097_152),
            ("0B", 0),
            ("34", 34),
        ] {
            assert_eq!(size_bytes(input), Some(expected));
        }
        for value in [
            "N/A",
            "",
            "-1B",
            "1XB",
            "1.2.3MB",
            "999999999999999999999TB",
        ] {
            assert_eq!(size_bytes(value), None);
        }
    }

    #[test]
    fn dangling_unknown_metadata_and_duplicate_reclaim_estimate() {
        let list = r#"{"ID":"abc123","Repository":"<none>","Tag":"<none>","Size":"1MB","CreatedAt":"2026-01-01"}"#;
        let mut images = parse(list, r#"{"Images":null}"#).unwrap();
        assert!(images[0].dangling());
        assert_eq!(images[0].remove_target(), "abc123");
        assert_eq!(reclaimable(&images), None);
        images[0].containers = Some(0);
        images[0].unique_size = Some(100);
        images.push(images[0].clone());
        assert_eq!(reclaimable(&images), Some(100));
        assert!(parse("not json", r#"{"Images":[]}"#).is_err());
        assert!(parse(list, "not json").is_err());
    }
}
