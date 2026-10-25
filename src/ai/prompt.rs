use std::path::Path;

use super::skill::RAW_URL;
use crate::colima::{Instance, Status, model::gib};

/// Shell-quote a profile/context so a copied command stays a single argument.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn build(instance: &Instance, status: Option<&Status>, skill: Option<&Path>) -> String {
    // Never reuse a cached running status for a stopped instance.
    let status = status.filter(|_| instance.is_running());
    let location = skill.map_or_else(
        || format!("{RAW_URL}/SKILL.md"),
        |path| path.join("SKILL.md").display().to_string(),
    );
    let runtime = status.map_or(instance.runtime.as_str(), |s| s.runtime.as_str());
    let arch = status.map_or(instance.arch.as_str(), |s| s.arch.as_str());
    let cpu = status.map_or(instance.cpus, |s| s.cpu);
    let memory = status.map_or(instance.memory, |s| s.memory);
    let disk = status.map_or(instance.disk, |s| s.disk);
    let socket = status
        .map(|s| s.docker_socket.as_str())
        .filter(|s| !s.is_empty());
    // Colima names both the Docker and the kubectl context `colima` or `colima-<profile>`.
    let context_name = if instance.name == "default" {
        "colima".to_owned()
    } else {
        format!("colima-{}", instance.name)
    };
    // `colima list` reports `docker+k3s` while Kubernetes is on.
    let kubernetes = status.map_or(instance.has_kubernetes(), |s| s.kubernetes);
    let kubernetes = if kubernetes {
        format!("On (k3s; kubectl context {})", quote(&context_name))
    } else {
        "Off".into()
    };
    let runtime = runtime.split('+').next().unwrap_or_default();
    let context = if runtime == "docker" {
        format!("docker context use {}", quote(&context_name))
    } else {
        "Not applicable (runtime is not Docker).".into()
    };
    format!(
        "Help me manage this Colima profile. Read the Colima agent skill at {location} first, including its references as needed.\n\nProfile: {}\nStatus: {}\nRuntime: {}\nArchitecture: {}\nCPUs: {cpu}\nMemory: {}\nDisk: {}\nDocker socket: {}\nDocker context command: {context}\nKubernetes: {kubernetes}\nProfile command flag: -p {}\n\nRules:\n1. Run `colima version` first and use advice appropriate to that version.\n2. Try risky changes on a throwaway profile first.\n3. Never stop, delete or prune anything without asking me first.",
        instance.name,
        instance.status,
        if runtime.is_empty() {
            "Unknown"
        } else {
            runtime
        },
        if arch.is_empty() { "Unknown" } else { arch },
        gib(memory),
        gib(disk),
        socket.unwrap_or("Unavailable (profile stopped or socket not reported)."),
        quote(&instance.name),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance() -> Instance {
        serde_json::from_str(r#"{"name":"gpui-test","status":"Running","runtime":"docker","arch":"aarch64","cpus":2,"memory":2147483648,"disk":107374182400}"#).unwrap()
    }

    #[test]
    fn remote_prompt_and_running_facts() {
        let prompt = build(&instance(), None, None);
        assert!(prompt.contains(&format!("{RAW_URL}/SKILL.md")));
        for fact in [
            "Profile: gpui-test",
            "Status: Running",
            "CPUs: 2",
            "Memory: 2 GiB",
            "Disk: 100 GiB",
            "docker context use 'colima-gpui-test'",
            "colima version",
            "throwaway profile",
            "Never stop, delete or prune",
        ] {
            assert!(prompt.contains(fact), "missing {fact}");
        }
    }

    #[test]
    fn kubernetes_profiles_keep_the_docker_context_and_name_the_kube_context() {
        // Stopped, so only `colima list`'s `docker+k3s` is known.
        let mut instance = instance();
        instance.status = "Stopped".into();
        instance.runtime = "docker+k3s".into();
        let prompt = build(&instance, None, None);
        assert!(prompt.contains("Runtime: docker\n"));
        assert!(prompt.contains("docker context use 'colima-gpui-test'"));
        assert!(prompt.contains("Kubernetes: On (k3s; kubectl context 'colima-gpui-test')"));
        assert!(build(&self::instance(), None, None).contains("Kubernetes: Off"));
    }

    #[test]
    fn local_prompt_and_stopped_facts() {
        let mut instance = instance();
        instance.status = "Stopped".into();
        let status: Status =
            serde_json::from_str(include_str!("../../tests/fixtures/colima_status.json")).unwrap();
        let prompt = build(
            &instance,
            Some(&status),
            Some(Path::new("/tmp/t49-skills/colima")),
        );
        assert!(prompt.contains("/tmp/t49-skills/colima/SKILL.md"));
        assert!(!prompt.contains(RAW_URL));
        assert!(prompt.contains("Status: Stopped"));
        assert!(prompt.contains("Docker socket: Unavailable"));
        instance.name = "default".into();
        instance.status = "Running".into();
        let prompt = build(&instance, Some(&status), None);
        assert!(prompt.contains(&status.docker_socket));
        assert!(prompt.contains("docker context use 'colima'"));
        instance.runtime = "containerd".into();
        assert!(build(&instance, None, None).contains("Not applicable"));
    }
}
