use serde::Deserialize;

/// One line of `colima list --json` (newline-delimited JSON, one object per profile).
///
/// Only `name` and `status` are always present: while a VM is first being created,
/// colima omits fields such as `runtime`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Instance {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub cpus: u32,
    /// Bytes.
    #[serde(default)]
    pub memory: u64,
    /// Bytes.
    #[serde(default)]
    pub disk: u64,
    /// The container runtime, with `+k3s` appended while Kubernetes is on
    /// (`docker+k3s`). Use [`Instance::container_runtime`] to compare it.
    #[serde(default)]
    pub runtime: String,
    #[serde(default)]
    pub address: String,
}

impl Instance {
    pub fn is_running(&self) -> bool {
        self.status == "Running"
    }

    /// `docker`, `containerd` or `incus`, without the Kubernetes suffix.
    pub fn container_runtime(&self) -> &str {
        self.runtime.split('+').next().unwrap_or_default()
    }

    /// Whether the profile uses Docker, so it has Docker tabs (with or without Kubernetes).
    pub fn is_docker(&self) -> bool {
        self.container_runtime() == "docker"
    }

    /// Whether Kubernetes (k3s) is enabled, from the runtime suffix in `colima list`.
    pub fn has_kubernetes(&self) -> bool {
        self.runtime
            .split('+')
            .skip(1)
            .any(|part| part.starts_with("k3s"))
    }
}

/// Output of `colima status --json -p <profile>`; only available while running.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Status {
    pub display_name: String,
    pub driver: String,
    pub arch: String,
    pub runtime: String,
    pub mount_type: String,
    #[serde(default)]
    pub ip_address: String,
    #[serde(default)]
    pub docker_socket: String,
    #[serde(default)]
    pub containerd_socket: String,
    pub kubernetes: bool,
    pub cpu: u32,
    /// Bytes.
    pub memory: u64,
    /// Bytes.
    pub disk: u64,
}

/// `colima version` output: the release and the git commit it was built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColimaVersion {
    /// e.g. `0.10.3`
    pub version: String,
    pub commit: Option<String>,
}

/// Formats a byte count as whole GiB, the unit colima's flags use.
pub fn gib(bytes: u64) -> String {
    format!("{} GiB", bytes / (1 << 30))
}

/// Editable scalar settings from Colima's generated configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartOptions {
    pub cpus: u32,
    pub memory_gib: u32,
    pub disk_gib: u32,
    pub runtime: String,
    pub vm_type: String,
    pub arch: String,
    pub kubernetes: bool,
    pub mount_type: String,
    pub network_address: bool,
    /// Switch the Docker/Kubernetes context to this profile on start. Not stored in
    /// `colima.yaml`; colima's own default is `true`.
    pub activate: bool,
}

impl Default for StartOptions {
    fn default() -> Self {
        Self {
            cpus: 2,
            memory_gib: 2,
            disk_gib: 100,
            runtime: "docker".into(),
            vm_type: "vz".into(),
            arch: "aarch64".into(),
            kubernetes: false,
            mount_type: "virtiofs".into(),
            network_address: false,
            activate: true,
        }
    }
}

impl StartOptions {
    pub fn to_args(&self) -> Vec<String> {
        [
            ("--cpus", self.cpus.to_string()),
            ("--memory", self.memory_gib.to_string()),
            ("--disk", self.disk_gib.to_string()),
            ("--runtime", self.runtime.clone()),
            ("--vm-type", self.vm_type.clone()),
            ("--arch", self.arch.clone()),
            ("--mount-type", self.mount_type.clone()),
        ]
        .into_iter()
        .flat_map(|(key, value)| [key.to_owned(), value])
        .chain([
            format!("--kubernetes={}", self.kubernetes),
            format!("--network-address={}", self.network_address),
            format!("--activate={}", self.activate),
        ])
        .collect()
    }

    pub fn read_config(profile: &str) -> Result<Self, super::CliError> {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| super::CliError::Parse("HOME is not set".into()))?;
        Self::read_config_at(std::path::Path::new(&home), profile)
    }

    pub fn read_config_at(home: &std::path::Path, profile: &str) -> Result<Self, super::CliError> {
        if profile.is_empty()
            || !profile
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(super::CliError::Parse("invalid profile name".into()));
        }
        let path = home.join(".colima").join(profile).join("colima.yaml");
        let text = std::fs::read_to_string(&path).map_err(|source| super::CliError::Spawn {
            program: path.display().to_string(),
            source,
        })?;
        Self::parse_config(&text)
    }

    /// Reads the scalar subset emitted by Colima, without adding a YAML dependency.
    /// Unknown sections (mounts, provision, env, etc.) are ignored; unsupported
    /// syntax in an editable field returns an error instead of silently changing it.
    pub fn parse_config(text: &str) -> Result<Self, super::CliError> {
        let mut options = Self::default();
        let mut section = "";
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let Some((key, raw)) = trimmed.split_once(':') else {
                continue;
            };
            let nested = line.starts_with(char::is_whitespace);
            if !nested {
                section = key;
            }
            let value = raw.split(" #").next().unwrap_or(raw).trim();
            let value = if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                &value[1..value.len() - 1]
            } else {
                value
            };
            let error =
                || super::CliError::Parse(format!("invalid config value for {key}: {value}"));
            let number = || value.parse::<u32>().map_err(|_| error());
            let boolean = || value.parse::<bool>().map_err(|_| error());
            let string = || {
                if value.is_empty()
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                {
                    Err(error())
                } else {
                    Ok(value.to_owned())
                }
            };
            match (nested, section, key) {
                (false, _, "cpu") => options.cpus = number()?,
                (false, _, "memory") => options.memory_gib = number()?,
                (false, _, "disk") => options.disk_gib = number()?,
                (false, _, "runtime") => options.runtime = string()?,
                (false, _, "vmType") => options.vm_type = string()?,
                (false, _, "arch") => options.arch = string()?,
                (false, _, "mountType") => options.mount_type = string()?,
                (true, "kubernetes", "enabled") => options.kubernetes = boolean()?,
                (true, "network", "address") => options.network_address = boolean()?,
                _ => {}
            }
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_and_all_flags() {
        let options =
            StartOptions::parse_config(include_str!("../../tests/fixtures/colima_config.yaml"))
                .unwrap();
        assert_eq!(
            options.to_args(),
            [
                "--cpus",
                "4",
                "--memory",
                "8",
                "--disk",
                "120",
                "--runtime",
                "containerd",
                "--vm-type",
                "qemu",
                "--arch",
                "x86_64",
                "--mount-type",
                "9p",
                "--kubernetes=true",
                "--network-address=true",
                "--activate=true"
            ]
        );
        assert!(
            StartOptions::default()
                .to_args()
                .contains(&"--kubernetes=false".into())
        );
        assert!(
            StartOptions::default()
                .to_args()
                .contains(&"--network-address=false".into())
        );
        assert!(StartOptions::parse_config("cpu: nope").is_err());
        assert!(StartOptions::read_config_at(std::path::Path::new("/tmp"), "../default").is_err());
    }
    #[test]
    fn existing_fixtures() {
        assert_eq!(
            super::super::commands::parse_list(include_str!(
                "../../tests/fixtures/colima_list.json"
            ))
            .unwrap()
            .len(),
            1
        );
        assert!(
            !super::super::commands::parse_status(include_str!(
                "../../tests/fixtures/colima_status.json"
            ))
            .unwrap()
            .docker_socket
            .is_empty()
        );
    }
}
