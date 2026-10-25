use colima_gpui::colima::commands::{parse_list, parse_status, parse_version};

#[test]
fn parses_list_fixture() {
    let instances = parse_list(include_str!("fixtures/colima_list.json")).unwrap();
    assert_eq!(instances.len(), 1);
    let default = &instances[0];
    assert_eq!(default.name, "default");
    assert!(default.is_running());
    assert_eq!(default.cpus, 2);
    assert_eq!(default.memory, 8 << 30);
    assert_eq!(default.address, "192.168.64.2");
}

#[test]
fn parses_profile_still_being_created() {
    // Shape seen while `colima start` provisions a new VM: no runtime yet.
    let line = r#"{"name":"gpui-test","status":"Stopped","arch":"aarch64","cpus":1,"memory":1073741824,"disk":10737418240}"#;
    let instances = parse_list(line).unwrap();
    assert_eq!(instances[0].name, "gpui-test");
    assert_eq!(instances[0].runtime, "");
}

#[test]
fn parses_version_fixture() {
    let version = parse_version(include_str!("fixtures/colima_version.txt")).unwrap();
    assert_eq!(version.version, "0.10.3");
    assert_eq!(
        version.commit.as_deref(),
        Some("00f6c297e92a82c04a4ab507db0a61435650d7e8")
    );
    assert!(parse_version("something else").is_err());
}

#[test]
fn parses_empty_list() {
    assert!(parse_list("\n").unwrap().is_empty());
}

#[test]
fn rejects_malformed_list() {
    assert!(parse_list("{not json}").is_err());
}

#[test]
fn parses_status_fixture() {
    let status = parse_status(include_str!("fixtures/colima_status.json")).unwrap();
    assert_eq!(status.runtime, "docker");
    assert_eq!(
        status.docker_socket,
        "unix:///Users/user/.colima/default/docker.sock"
    );
    assert!(!status.kubernetes);
}

#[test]
fn kubernetes_profile_is_still_a_docker_profile() {
    // Captured from a docker profile with Kubernetes on: `colima list` appends `+k3s`.
    let instances = parse_list(include_str!("fixtures/colima_list_k3s.json")).unwrap();
    let k3s = &instances[0];
    assert_eq!(k3s.runtime, "docker+k3s");
    assert_eq!(k3s.container_runtime(), "docker");
    assert!(k3s.is_docker());
    assert!(k3s.has_kubernetes());

    let plain = &parse_list(include_str!("fixtures/colima_list.json")).unwrap()[0];
    assert!(plain.is_docker() && !plain.has_kubernetes());

    let status = parse_status(include_str!("fixtures/colima_status_k3s.json")).unwrap();
    assert!(status.kubernetes);
    assert_eq!(status.runtime, "docker");
}

#[test]
fn containerd_profile_has_no_docker_socket() {
    // Captured from a containerd profile: no `docker_socket`, plus `buildkitd_socket`.
    let instance = &parse_list(include_str!("fixtures/colima_list_containerd.json")).unwrap()[0];
    assert_eq!(instance.container_runtime(), "containerd");
    assert!(!instance.is_docker() && !instance.has_kubernetes());

    let status = parse_status(include_str!("fixtures/colima_status_containerd.json")).unwrap();
    assert_eq!(status.runtime, "containerd");
    assert!(status.docker_socket.is_empty());
    assert!(status.containerd_socket.ends_with("containerd.sock"));
}
