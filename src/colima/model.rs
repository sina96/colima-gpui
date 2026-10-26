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

/// A host folder mounted into the VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub location: String,
    pub writable: bool,
    /// Preserve CLI-created mounts with a different guest path when editing.
    pub mount_point: String,
}

/// Editable settings from Colima's generated configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartOptions {
    pub cpus: u32,
    pub memory_gib: u32,
    pub disk_gib: u32,
    pub runtime: String,
    pub vm_type: String,
    pub arch: String,
    pub kubernetes: bool,
    /// Empty uses Colima's default k3s version.
    pub kubernetes_version: String,
    pub rosetta: bool,
    pub mount_type: String,
    /// None disables mounts; Some(empty) uses Colima's writable home mount.
    pub mounts: Option<Vec<Mount>>,
    pub mount_inotify: bool,
    pub hostname: String,
    pub env: std::collections::BTreeMap<String, String>,
    pub ssh_agent: bool,
    pub root_disk_gib: u32,
    pub cpu_type: String,
    /// None omits the flag on hosts where Colima does not register it.
    pub nested_virtualization: Option<bool>,
    pub port_forwarder: String,
    pub k3s_args: Vec<String>,
    pub network_address: bool,
    /// Empty uses Colima's DNS resolver.
    pub dns: Vec<String>,
    pub network_mode: String,
    pub network_interface: String,
    /// Switch the Docker/Kubernetes context to this profile on start. Not stored in
    /// `colima.yaml`; colima's own default is `true`.
    pub activate: bool,
}

impl Default for StartOptions {
    fn default() -> Self {
        host_defaults(std::env::consts::OS, std::env::consts::ARCH)
    }
}

/// Defaults for a new profile on the given host, independent of the running platform.
pub fn host_defaults(os: &str, arch: &str) -> StartOptions {
    let linux = os == "linux";
    StartOptions {
        cpus: 2,
        memory_gib: 2,
        disk_gib: 100,
        runtime: "docker".into(),
        vm_type: if linux { "qemu" } else { "vz" }.into(),
        arch: if arch == "x86_64" {
            "x86_64"
        } else {
            "aarch64"
        }
        .into(),
        kubernetes: false,
        kubernetes_version: String::new(),
        rosetta: false,
        mount_type: if linux { "sshfs" } else { "virtiofs" }.into(),
        mounts: Some(Vec::new()),
        mount_inotify: false,
        hostname: String::new(),
        env: Default::default(),
        ssh_agent: false,
        root_disk_gib: 20,
        cpu_type: String::new(),
        nested_virtualization: None,
        port_forwarder: "ssh".into(),
        k3s_args: vec!["--disable=traefik".into()],
        network_address: false,
        dns: Vec::new(),
        network_mode: "shared".into(),
        network_interface: "en0".into(),
        activate: true,
    }
}

impl StartOptions {
    pub fn to_args(&self) -> Vec<String> {
        let mut args: Vec<String> = [
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
        .collect();
        // Colima only registers this flag on Apple silicon Macs.
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            args.push(format!(
                "--vz-rosetta={}",
                self.rosetta && self.vm_type == "vz"
            ));
        }
        // An explicit empty value resets a saved version to Colima's default.
        // Omitting the flag would keep the old version from colima.yaml.
        args.extend([
            "--kubernetes-version".into(),
            self.kubernetes_version.clone(),
        ]);
        // An explicit empty flag resets a saved list to Colima's default.
        match &self.mounts {
            None => args.extend(["--mount".into(), "none".into()]),
            Some(mounts) if mounts.is_empty() => args.extend(["--mount".into(), String::new()]),
            Some(mounts) => {
                for mount in mounts {
                    let mut value = mount.location.clone();
                    if !mount.mount_point.is_empty() {
                        value.push(':');
                        value.push_str(&mount.mount_point);
                    }
                    if mount.writable {
                        value.push_str(":w");
                    }
                    // Cobra's StringSlice flag parses CSV, even with repeated flags.
                    if value.contains([',', '"']) {
                        value = format!("\"{}\"", value.replace('"', "\"\""));
                    }
                    args.extend(["--mount".into(), value]);
                }
            }
        }
        // Send an explicit empty slice to clear previously saved DNS servers.
        if self.dns.is_empty() {
            args.extend(["--dns".into(), String::new()]);
        } else {
            for server in &self.dns {
                args.extend(["--dns".into(), server.clone()]);
            }
        }
        // Colima registers mode/interface only on macOS. Preserve the saved
        // interface even when shared mode hides its form field.
        if cfg!(target_os = "macos") {
            args.extend([
                "--network-mode".into(),
                self.network_mode.clone(),
                "--network-interface".into(),
                self.network_interface.clone(),
            ]);
        }
        args.extend([
            format!("--mount-inotify={}", self.mount_inotify),
            format!("--ssh-agent={}", self.ssh_agent),
            "--hostname".into(),
            self.hostname.clone(),
            "--root-disk".into(),
            self.root_disk_gib.to_string(),
            "--port-forwarder".into(),
            self.port_forwarder.clone(),
        ]);
        if self.vm_type == "qemu" {
            args.extend(["--cpu-type".into(), self.cpu_type.clone()]);
        }
        if let Some(enabled) = self.nested_virtualization {
            args.push(format!("--nested-virtualization={enabled}"));
        }
        for (key, value) in &self.env {
            let pair = format!("{key}={value}");
            // pflag strips surrounding quotes when a value has only one '='.
            // Repeating the same CSV field forces its CSV branch without changing
            // the resulting map, preserving trailing quotes and embedded commas.
            let record = csv_value(&pair);
            let pair = format!("{record},{record}");
            args.extend(["--env".into(), pair]);
        }
        if self.k3s_args.is_empty() {
            args.extend(["--k3s-arg".into(), String::new()]);
        } else {
            for arg in &self.k3s_args {
                args.extend(["--k3s-arg".into(), csv_value(arg)]);
            }
        }
        args
    }

    pub fn read_config(profile: &str) -> Result<Self, super::CliError> {
        let home = super::paths::colima_home()
            .ok_or_else(|| super::CliError::Parse("HOME is not set".into()))?;
        Self::read_config_at(&home, profile)
    }

    /// `colima_home` is Colima's config directory (see [`super::paths::colima_home`]).
    pub fn read_config_at(
        colima_home: &std::path::Path,
        profile: &str,
    ) -> Result<Self, super::CliError> {
        if !crate::docker::system::valid_profile(profile) {
            return Err(super::CliError::Parse("invalid profile name".into()));
        }
        let path = colima_home.join(profile).join("colima.yaml");
        let text = std::fs::read_to_string(&path).map_err(|source| super::CliError::Spawn {
            program: path.display().to_string(),
            source,
        })?;
        Self::parse_config(&text)
    }

    /// Reads the editable subset emitted by Colima, without adding a YAML dependency.
    /// Unknown sections (provision, docker, etc.) are ignored; unsupported
    /// syntax in an editable field returns an error instead of silently changing it.
    pub fn parse_config(text: &str) -> Result<Self, super::CliError> {
        let mut options = Self {
            mounts: parse_mounts(text)?,
            dns: parse_dns(text)?,
            ..Self::default()
        };
        parse_advanced_collections(text, &mut options)?;
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
            let value = if value.len() >= 2
                && ((value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\'')))
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
                (false, _, "mountInotify") => options.mount_inotify = boolean()?,
                (false, _, "forwardAgent") => options.ssh_agent = boolean()?,
                (false, _, "nestedVirtualization") => {
                    options.nested_virtualization = Some(boolean()?)
                }
                (false, _, "rootDisk") => options.root_disk_gib = number()?,
                (false, _, "hostname") => options.hostname = optional_scalar(raw)?,
                (false, _, "cpuType") => options.cpu_type = optional_scalar(raw)?,
                (false, _, "portForwarder") => options.port_forwarder = string()?,
                (false, _, "cpu") => options.cpus = number()?,
                (false, _, "memory") => options.memory_gib = number()?,
                (false, _, "disk") => options.disk_gib = number()?,
                (false, _, "runtime") => options.runtime = string()?,
                (false, _, "vmType") => options.vm_type = string()?,
                (false, _, "arch") => options.arch = string()?,
                (false, _, "mountType") => options.mount_type = string()?,
                (false, _, "rosetta") => options.rosetta = boolean()?,
                (true, "kubernetes", "enabled") => options.kubernetes = boolean()?,
                (true, "kubernetes", "version") => {
                    // k3s versions include a build suffix, e.g. v1.35.0+k3s1.
                    if !value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.+".contains(&b))
                    {
                        return Err(error());
                    }
                    options.kubernetes_version = value.to_owned();
                }
                (true, "network", "address") => options.network_address = boolean()?,
                (true, "network", "mode") => {
                    if !matches!(value, "shared" | "bridged") {
                        return Err(error());
                    }
                    options.network_mode = value.to_owned();
                }
                (true, "network", "interface") => options.network_interface = string()?,
                _ => {}
            }
        }
        Ok(options)
    }
}

// Cobra StringSlice flags parse each occurrence as a CSV record.
fn csv_value(value: &str) -> String {
    if value.contains([',', '"']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn optional_scalar(raw: &str) -> Result<String, super::CliError> {
    let value = raw.split(" #").next().unwrap_or(raw).trim();
    if matches!(value, "" | "null" | "~") {
        Ok(String::new())
    } else {
        mount_scalar(raw)
    }
}

/// Only the block map/list shapes emitted by Colima are supported. Reject other
/// YAML shapes rather than silently dropping environment variables or k3s args.
fn parse_advanced_collections(
    text: &str,
    options: &mut StartOptions,
) -> Result<(), super::CliError> {
    let error =
        || super::CliError::Parse("unsupported env or kubernetes.k3sArgs configuration".into());
    let mut section = "";
    let mut list_indent = None;
    let mut env_indent = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if let Some(level) = list_indent {
            if indent >= level
                && let Some(raw) = trimmed.strip_prefix("- ")
            {
                let value = mount_scalar(raw)?;
                if value.chars().any(char::is_control) {
                    return Err(error());
                }
                options.k3s_args.push(value);
                continue;
            }
            if indent > level {
                return Err(error());
            }
            list_indent = None;
        }
        if indent == 0 {
            let Some((key, raw)) = trimmed.split_once(':') else {
                continue;
            };
            section = key;
            env_indent = None;
            if key == "env" {
                let raw = raw.split(" #").next().unwrap_or(raw).trim();
                if !matches!(raw, "" | "{}" | "null" | "~") {
                    return Err(error());
                }
            }
        } else if section == "env" {
            if *env_indent.get_or_insert(indent) != indent {
                return Err(error());
            }
            let (key, raw) = trimmed.split_once(':').ok_or_else(error)?;
            let key = mount_scalar(key)?;
            let value = optional_scalar(raw)?;
            if key.chars().any(char::is_control)
                || value.chars().any(char::is_control)
                || options.env.insert(key, value).is_some()
            {
                return Err(error());
            }
        }
        if section == "kubernetes"
            && indent > 0
            && let Some(raw) = trimmed.strip_prefix("k3sArgs:")
        {
            options.k3s_args.clear();
            let raw = raw.split(" #").next().unwrap_or(raw).trim();
            match raw {
                "" => list_indent = Some(indent),
                "[]" | "null" | "~" => {}
                _ => return Err(error()),
            }
        }
    }
    Ok(())
}

/// Colima emits DNS as a block IP list (or [] when using its resolver).
fn parse_dns(text: &str) -> Result<Vec<String>, super::CliError> {
    let error = || super::CliError::Parse("unsupported network.dns configuration".into());
    let mut network = false;
    let mut dns_indent = None;
    let mut servers = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent == 0 && !trimmed.starts_with('-') {
            network = trimmed == "network:" || trimmed.starts_with("network: #");
            dns_indent = None;
            continue;
        }
        if !network {
            continue;
        }
        if let Some(level) = dns_indent {
            if trimmed.starts_with("- ") && indent >= level {
                let value = mount_scalar(&trimmed[2..])?;
                value.parse::<std::net::IpAddr>().map_err(|_| error())?;
                servers.push(value);
                continue;
            }
            if indent > level {
                return Err(error());
            }
            dns_indent = None;
        }
        if let Some(raw) = trimmed.strip_prefix("dns:") {
            let raw = raw.split(" #").next().unwrap_or(raw).trim();
            match raw {
                "" => dns_indent = Some(indent),
                "[]" | "null" | "~" => {}
                value if value.starts_with('[') && value.ends_with(']') => {
                    for raw in value[1..value.len() - 1].split(',') {
                        let value = mount_scalar(raw)?;
                        value.parse::<std::net::IpAddr>().map_err(|_| error())?;
                        servers.push(value);
                    }
                }
                _ => return Err(error()),
            }
        }
    }
    Ok(servers)
}

/// Colima emits a block list, [] for defaults, and null for disabled mounts.
fn parse_mounts(text: &str) -> Result<Option<Vec<Mount>>, super::CliError> {
    let error = || super::CliError::Parse("unsupported mounts configuration".into());
    let mut mounts = Vec::<Mount>::new();
    let mut in_mounts = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !line.starts_with(char::is_whitespace) && !trimmed.starts_with('-') {
            if let Some(raw) = trimmed.strip_prefix("mounts:") {
                let value = raw.split(" #").next().unwrap_or(raw).trim();
                match value {
                    "[]" => return Ok(Some(Vec::new())),
                    "null" | "~" => return Ok(None),
                    "" => in_mounts = true,
                    _ => return Err(error()),
                }
                continue;
            }
            if in_mounts {
                break;
            }
        }
        if !in_mounts {
            continue;
        }
        let field = if let Some(field) = trimmed.strip_prefix("- ") {
            mounts.push(Mount {
                location: String::new(),
                writable: false,
                mount_point: String::new(),
            });
            field
        } else {
            trimmed
        };
        let mount = mounts.last_mut().ok_or_else(error)?;
        let (key, raw) = field.split_once(':').ok_or_else(error)?;
        let value = mount_scalar(raw)?;
        match key.trim() {
            "location" => mount.location = value,
            "writable" => mount.writable = value.parse().map_err(|_| error())?,
            "mountPoint" => mount.mount_point = value,
            _ => return Err(error()),
        }
    }
    if mounts.iter().any(|mount| mount.location.is_empty()) {
        return Err(error());
    }
    if in_mounts && mounts.is_empty() {
        Ok(None)
    } else {
        Ok(Some(mounts))
    }
}

fn mount_scalar(raw: &str) -> Result<String, super::CliError> {
    let raw = raw.trim();
    let error = || super::CliError::Parse("unsupported mount value".into());
    if raw.starts_with('"') || raw.starts_with('\'') {
        let quote = raw.chars().next().unwrap();
        // Find a closing quote followed only by whitespace or a YAML comment.
        for (index, ch) in raw.char_indices().skip(1) {
            if ch != quote {
                continue;
            }
            let rest = raw[index + 1..].trim();
            if !rest.is_empty() && !rest.starts_with('#') {
                continue;
            }
            let quoted = &raw[..=index];
            return if quote == '"' {
                serde_json::from_str(quoted).map_err(|_| error())
            } else {
                Ok(raw[1..index].replace("''", "'"))
            };
        }
        return Err(error());
    }
    let value = raw.split(" #").next().unwrap_or(raw).trim();
    if value.is_empty() || value.starts_with(['[', '{', '&', '*', '!', '|', '>']) {
        return Err(error());
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advanced_config_and_csv_flags() {
        let fixture = include_str!("../../tests/fixtures/colima_config.yaml");
        let config = format!(
            "{fixture}\n{}",
            r#"mountInotify: true
hostname: t65-vm.example
forwardAgent: true
rootDisk: 30
cpuType: cortex-a72
nestedVirtualization: true
portForwarder: grpc
env:
  SIMPLE: value
  QUOTED: 'comma, equals= and "quote"'
  EMPTY: ""
  TRAILING: 'value"'
"#
        )
        .replace(
            "  enabled: true",
            r#"  enabled: true
  k3sArgs:
    - --disable=traefik,servicelb
    - '--node-label=example=hello world'"#,
        );
        let options = StartOptions::parse_config(&config).unwrap();
        assert!(options.mount_inotify && options.ssh_agent);
        assert_eq!(options.hostname, "t65-vm.example");
        assert_eq!(options.root_disk_gib, 30);
        assert_eq!(options.cpu_type, "cortex-a72");
        assert_eq!(options.nested_virtualization, Some(true));
        assert_eq!(options.port_forwarder, "grpc");
        assert_eq!(options.env["EMPTY"], "");
        assert_eq!(options.env["QUOTED"], "comma, equals= and \"quote\"");
        assert_eq!(
            options.k3s_args,
            [
                "--disable=traefik,servicelb",
                "--node-label=example=hello world"
            ]
        );
        let args = options.to_args();
        assert!(args.contains(&"--nested-virtualization=true".into()));
        assert!(args.contains(&"--mount-inotify=true".into()));
        assert!(args.contains(&"--ssh-agent=true".into()));
        for (flag, value) in [
            ("--hostname", "t65-vm.example"),
            ("--root-disk", "30"),
            ("--cpu-type", "cortex-a72"),
            ("--port-forwarder", "grpc"),
        ] {
            assert!(args.windows(2).any(|pair| pair == [flag, value]));
        }
        let env: Vec<_> = args
            .windows(2)
            .filter(|p| p[0] == "--env")
            .map(|p| p[1].as_str())
            .collect();
        assert_eq!(
            env,
            [
                "EMPTY=,EMPTY=",
                "\"QUOTED=comma, equals= and \"\"quote\"\"\",\"QUOTED=comma, equals= and \"\"quote\"\"\"",
                "SIMPLE=value,SIMPLE=value",
                "\"TRAILING=value\"\"\",\"TRAILING=value\"\"\""
            ]
        );
        assert!(
            args.windows(2)
                .any(|p| p == ["--k3s-arg", "\"--disable=traefik,servicelb\""])
        );
        let cleared = StartOptions::parse_config(
            "env: {}\nkubernetes:\n  k3sArgs: []\nhostname: \"\"\ncpuType: \"\"",
        )
        .unwrap();
        assert!(cleared.env.is_empty() && cleared.k3s_args.is_empty());
        assert!(cleared.to_args().windows(2).any(|p| p == ["--k3s-arg", ""]));
        assert!(
            !StartOptions::default()
                .to_args()
                .iter()
                .any(|a| a.starts_with("--nested-virtualization"))
        );
        let mut disabled = options.clone();
        disabled.nested_virtualization = Some(false);
        disabled.mount_inotify = false;
        disabled.ssh_agent = false;
        for flag in [
            "--nested-virtualization=false",
            "--mount-inotify=false",
            "--ssh-agent=false",
        ] {
            assert!(disabled.to_args().contains(&flag.into()));
        }
        let mut vz = options;
        vz.vm_type = "vz".into();
        assert!(!vz.to_args().contains(&"--cpu-type".into()));
    }

    #[test]
    fn rejects_unsupported_advanced_yaml_without_losing_values() {
        for config in [
            "mountInotify: yes",
            "forwardAgent: yes",
            "nestedVirtualization: yes",
            "rootDisk: nope",
            "env: {KEY: value}",
            "env:\n  KEY: [one, two]",
            "env:\n  KEY: first\n  KEY: second",
            "env:\n  KEY: |\n    multiline",
            "env:\n  KEY:\n    NESTED: value",
            r#"env:
  KEY: "line\nfeed""#,
            "kubernetes:\n  k3sArgs: [--disable=traefik]",
            "kubernetes:\n  k3sArgs:\n    - \"unterminated",
        ] {
            assert!(StartOptions::parse_config(config).is_err(), "{config}");
        }
    }

    #[test]
    fn defaults_for_each_host() {
        for (os, arch, vm_type, mount_type) in [
            ("linux", "x86_64", "qemu", "sshfs"),
            ("linux", "aarch64", "qemu", "sshfs"),
            ("macos", "x86_64", "vz", "virtiofs"),
            ("macos", "aarch64", "vz", "virtiofs"),
        ] {
            let options = host_defaults(os, arch);
            assert_eq!(options.arch, arch);
            assert_eq!(options.vm_type, vm_type);
            assert_eq!(options.mount_type, mount_type);
            let args = options.to_args();
            assert!(args.windows(2).any(|pair| pair == ["--vm-type", vm_type]));
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--mount-type", mount_type])
            );
        }
    }

    #[test]
    fn reads_cli_profile_names_without_allowing_path_traversal() {
        let home = std::env::temp_dir().join(format!("t59-config-{}", std::process::id()));
        std::fs::create_dir(&home).unwrap();
        for profile in ["my_vm", "Dev", "dev.test"] {
            let directory = home.join(profile);
            std::fs::create_dir(&directory).unwrap();
            std::fs::write(
                directory.join("colima.yaml"),
                include_str!("../../tests/fixtures/colima_config.yaml"),
            )
            .unwrap();
            let options = StartOptions::read_config_at(&home, profile).unwrap();
            assert_eq!(options.cpus, 4);
            assert_eq!(options.mount_type, "9p");
        }
        for profile in [
            "",
            ".",
            "..",
            "../default",
            "/default",
            "dev/nested",
            "dev\\nested",
        ] {
            assert!(matches!(
                StartOptions::read_config_at(&home, profile),
                Err(super::super::CliError::Parse(_))
            ));
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn config_and_all_flags() {
        let options =
            StartOptions::parse_config(include_str!("../../tests/fixtures/colima_config.yaml"))
                .unwrap();
        let mut expected = vec![
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
            "--activate=true",
        ];
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            expected.push("--vz-rosetta=false");
        }
        expected.extend(["--kubernetes-version", "", "--mount", "~", "--dns", ""]);
        if cfg!(target_os = "macos") {
            expected.extend(["--network-mode", "shared", "--network-interface", "en0"]);
        }
        expected.extend([
            "--mount-inotify=false",
            "--ssh-agent=false",
            "--hostname",
            "",
            "--root-disk",
            "20",
            "--port-forwarder",
            "ssh",
            "--cpu-type",
            "",
            "--k3s-arg",
            "--disable=traefik",
        ]);
        assert_eq!(options.to_args(), expected);
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
    fn rosetta_and_nested_kubernetes_version() {
        let config = include_str!("../../tests/fixtures/colima_config.yaml")
            .replace("vmType: qemu", "vmType: vz\nrosetta: true")
            .replace(
                "  enabled: true",
                "  enabled: true\n  version: 'v1.35.0+k3s1'",
            );
        let mut options = StartOptions::parse_config(&config).unwrap();
        assert!(options.rosetta);
        assert_eq!(options.kubernetes_version, "v1.35.0+k3s1");
        let supports_rosetta = cfg!(all(target_os = "macos", target_arch = "aarch64"));
        assert_eq!(
            options.to_args().contains(&"--vz-rosetta=true".into()),
            supports_rosetta
        );
        assert!(
            options
                .to_args()
                .windows(2)
                .any(|pair| { pair == ["--kubernetes-version", "v1.35.0+k3s1"] })
        );
        // Preserve the version even when Kubernetes is disabled for this start.
        options.kubernetes = false;
        assert!(options.to_args().contains(&"v1.35.0+k3s1".into()));
        options.vm_type = "qemu".into();
        assert_eq!(
            options.to_args().contains(&"--vz-rosetta=false".into()),
            supports_rosetta
        );
        for value in ["", "\"\"", "''"] {
            let options = StartOptions::parse_config(&format!(
                "rosetta: false\nkubernetes:\n  version: {value}\n"
            ))
            .unwrap();
            assert!(!options.rosetta);
            assert!(options.kubernetes_version.is_empty());
            assert!(
                options
                    .to_args()
                    .windows(2)
                    .any(|pair| pair == ["--kubernetes-version", ""])
            );
        }
        assert!(StartOptions::parse_config("rosetta: yes").is_err());
        assert!(StartOptions::parse_config("kubernetes:\n  version: [v1.35.0+k3s1]").is_err());
        let options =
            StartOptions::parse_config("version: ignored\nnetwork:\n  version: ignored").unwrap();
        assert!(options.kubernetes_version.is_empty());
    }
    #[test]
    fn mounts_config_and_flag_round_trip() {
        // Extend the captured generated config with Colima's block-list shape.
        let fixture = include_str!("../../tests/fixtures/colima_config.yaml");
        let config = fixture.replace(
            "  - location: ~",
            r#"  - location: ~/projects
    writable: true
  - location: '/tmp/read only # folder'
    writable: false
  - location: "/tmp/comma, and \"quote\""
    mountPoint: /mnt/files
    writable: true"#,
        );
        let options = StartOptions::parse_config(&config).unwrap();
        let mounts = options.mounts.as_ref().unwrap();
        assert_eq!(mounts.len(), 3);
        assert_eq!(mounts[0].location, "~/projects");
        assert!(mounts[0].writable);
        assert_eq!(mounts[1].location, "/tmp/read only # folder");
        assert!(!mounts[1].writable);
        let args = options.to_args();
        let flags: Vec<_> = args
            .windows(2)
            .filter(|pair| pair[0] == "--mount")
            .map(|pair| pair[1].as_str())
            .collect();
        assert_eq!(
            flags,
            [
                "~/projects:w",
                "/tmp/read only # folder",
                "\"/tmp/comma, and \"\"quote\"\":/mnt/files:w\""
            ]
        );
        assert_eq!(
            StartOptions::parse_config(
                "mounts:\n- location: '/tmp/it''s here'\n  writable: true\ncpu: 4"
            )
            .unwrap()
            .mounts
            .unwrap()[0]
                .location,
            "/tmp/it's here"
        );
    }

    #[test]
    fn mounts_defaults_disabled_and_invalid_config() {
        for config in ["", "mounts: []", "mounts: [] # defaults"] {
            let options = StartOptions::parse_config(config).unwrap();
            assert_eq!(options.mounts, Some(Vec::new()));
            assert!(
                options
                    .to_args()
                    .windows(2)
                    .any(|pair| pair == ["--mount", ""])
            );
        }
        for config in ["mounts: null", "mounts: ~", "mounts:\ncpu: 2"] {
            let options = StartOptions::parse_config(config).unwrap();
            assert_eq!(options.mounts, None);
            assert!(
                options
                    .to_args()
                    .windows(2)
                    .any(|pair| pair == ["--mount", "none"])
            );
        }
        for config in [
            "mounts: [{location: /tmp}]",
            "mounts:\n  - writable: true",
            "mounts:\n  - location: /tmp\n    writable: yes",
            "mounts:\n  - location: [one, two]",
            "mounts:\n  - location: \"unterminated",
            "mounts:\n  - location: /tmp\n    unknown: value",
        ] {
            assert!(StartOptions::parse_config(config).is_err(), "{config}");
        }
    }

    #[test]
    fn network_config_and_flags() {
        // Extend the captured configuration with Colima's generated network shape.
        let config = include_str!("../../tests/fixtures/colima_config.yaml").replace(
            "  address: true",
            "  address: true\n  mode: bridged\n  interface: en1\n  dns:\n    - 1.1.1.1\n    - 2606:4700:4700::1111\n  dnsHosts:\n    example.com: 192.0.2.1",
        );
        let options = StartOptions::parse_config(&config).unwrap();
        assert_eq!(options.dns, ["1.1.1.1", "2606:4700:4700::1111"]);
        assert_eq!(options.network_mode, "bridged");
        assert_eq!(options.network_interface, "en1");
        let args = options.to_args();
        let servers: Vec<_> = args
            .windows(2)
            .filter(|pair| pair[0] == "--dns")
            .map(|pair| pair[1].as_str())
            .collect();
        assert_eq!(servers, ["1.1.1.1", "2606:4700:4700::1111"]);
        assert_eq!(
            args.windows(2)
                .any(|pair| pair == ["--network-mode", "bridged"]),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            args.windows(2)
                .any(|pair| pair == ["--network-interface", "en1"]),
            cfg!(target_os = "macos")
        );
        for value in ["[]", "null", "~", ""] {
            let options =
                StartOptions::parse_config(&format!("network:\n  dns: {value}\n  address: false"))
                    .unwrap();
            assert!(options.dns.is_empty());
            assert!(
                options
                    .to_args()
                    .windows(2)
                    .any(|pair| pair == ["--dns", ""])
            );
        }
        for value in ["[1.1.1.1, '::1']", "\n  - 1.1.1.1\n  - '::1'"] {
            let options =
                StartOptions::parse_config(&format!("network:\n  dns: {value}\n  mode: shared"))
                    .unwrap();
            assert_eq!(options.dns, ["1.1.1.1", "::1"]);
        }
        for config in [
            "network:\n  dns: [example.com]",
            "network:\n  dns:\n    - 999.1.1.1",
            "network:\n  dns: 1.1.1.1",
            "network:\n  mode: invalid",
            "network:\n  interface: [en0]",
        ] {
            assert!(StartOptions::parse_config(config).is_err(), "{config}");
        }
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
