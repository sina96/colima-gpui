use serde::Deserialize;

/// Docker's JSON formatter exposes display strings, including Ports and CreatedAt.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Container {
    #[serde(rename = "ID")]
    pub id: String,
    pub names: String,
    pub image: String,
    pub state: String,
    pub status: String,
    #[serde(default)]
    pub ports: String,
    pub created_at: String,
    #[serde(default)]
    pub command: String,
}

impl Container {
    /// Whether Kubernetes created this container. With Kubernetes on, k3s runs pods on the
    /// Docker runtime through cri-dockerd, which names every container
    /// `k8s_<container>_<pod>_<namespace>_<uid>_<attempt>` (`k8s_POD_…` for the pause
    /// container that holds each pod's network).
    pub fn is_kubernetes(&self) -> bool {
        self.names
            .split(',')
            .any(|name| name.trim_start_matches('/').starts_with("k8s_"))
    }
}

#[cfg(test)]
mod kubernetes_tests {
    use super::*;

    fn container(names: &str) -> Container {
        Container {
            id: "abc".into(),
            names: names.into(),
            image: "x".into(),
            state: "running".into(),
            status: "Up".into(),
            ports: String::new(),
            created_at: String::new(),
            command: String::new(),
        }
    }

    #[test]
    fn kubernetes_containers_are_recognised_by_their_k8s_prefix() {
        // Names captured from a Colima profile with Kubernetes on.
        for name in [
            "k8s_coredns_coredns-54bf7cdff9-lp6vs_kube-system_51c335a2-b55e-4cbd-88cc-577310b21c63_0",
            "k8s_POD_coredns-54bf7cdff9-lp6vs_kube-system_51c335a2-b55e-4cbd-88cc-577310b21c63_0",
        ] {
            assert!(container(name).is_kubernetes(), "{name}");
        }
        for name in ["demo-grafana", "my_k8s_tool", "k8s", ""] {
            assert!(!container(name).is_kubernetes(), "{name}");
        }
    }
}
