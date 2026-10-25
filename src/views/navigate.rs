//! Cross-links between detail pages: a container's image and volumes, an image's or a
//! volume's containers.
//!
//! A Docker tab view emits [`Navigate`]; `ProfileDetail` switches to the target's tab and
//! calls that view's `open_target(key, cx)`. The view opens the item once it's listed, so
//! it works on a view created a moment ago. Back then returns to that tab's list.
//!
//! In a tab view: `cx.emit(Navigate(Target::Image(container.image.clone())))`.

/// What to open. Keys are what Docker shows elsewhere, matched loosely by the helpers below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Container ID (short or full) or name.
    Container(String),
    /// Image ID (with or without `sha256:`, short or full) or reference (`nginx`, `repo:tag`).
    Image(String),
    /// Volume name.
    Volume(String),
}

#[derive(Clone, Debug)]
pub struct Navigate(pub Target);

/// Whether a container with `id` and `names` (`docker ps` joins several with commas) is the
/// one `key` refers to.
pub fn container_matches(id: &str, names: &str, key: &str) -> bool {
    let key = key.trim_start_matches('/');
    names.split(',').any(|name| name.trim() == key) || id_matches(id, key)
}

/// Whether an image listed as `id` with `repository:tag` is the one `key` refers to.
pub fn image_matches(id: &str, repository: &str, tag: &str, key: &str) -> bool {
    if id_matches(id, key) {
        return true;
    }
    let listed = normalize_reference(&format!("{repository}:{tag}"));
    listed == normalize_reference(key)
}

/// IDs match by prefix either way, since `docker ps` and `image ls` print 12-character IDs
/// and `inspect` prints full ones. Needs at least 12 hex digits so a name can't match.
fn id_matches(id: &str, key: &str) -> bool {
    let id = id.trim_start_matches("sha256:");
    let key = key.trim_start_matches("sha256:");
    let hex = |s: &str| s.len() >= 12 && s.chars().all(|c| c.is_ascii_hexdigit());
    hex(id) && hex(key) && (id.starts_with(key) || key.starts_with(id))
}

/// `nginx` → `nginx:latest`, `docker.io/library/nginx:1` → `nginx:1`. Digests are kept.
fn normalize_reference(reference: &str) -> String {
    let reference = reference
        .trim_start_matches("docker.io/")
        .trim_start_matches("library/");
    let name_start = reference.rfind('/').map_or(0, |slash| slash + 1);
    if reference.contains('@') || reference[name_start..].contains(':') {
        reference.to_owned()
    } else {
        format!("{reference}:latest")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containers_match_by_name_or_either_id_length() {
        let id = "da327c9c1bec";
        let full = "da327c9c1beca6875b24fb3ae1fccfeccdd4bc6aca27504728e21cdca42502b3";
        assert!(container_matches(id, "demo-ministack", "demo-ministack"));
        assert!(container_matches(id, "demo-ministack", "/demo-ministack"));
        assert!(container_matches(id, "x", full));
        assert!(container_matches(full, "x", id));
        assert!(!container_matches(id, "x", "da32"));
        assert!(!container_matches(id, "x", "other"));
        assert!(container_matches(id, "web,web-alias", "web-alias"));
    }

    #[test]
    fn images_match_by_reference_or_id() {
        let id = "57b5ab1ca661";
        assert!(image_matches(
            id,
            "grafana/otel-lgtm",
            "latest",
            "grafana/otel-lgtm"
        ));
        assert!(image_matches(
            id,
            "grafana/otel-lgtm",
            "latest",
            "grafana/otel-lgtm:latest"
        ));
        assert!(image_matches(
            id,
            "nginx",
            "1.27",
            "docker.io/library/nginx:1.27"
        ));
        assert!(image_matches(
            id,
            "x",
            "y",
            "sha256:57b5ab1ca661ffeeddccbbaa"
        ));
        assert!(image_matches(
            id,
            "localhost:5000/app",
            "latest",
            "localhost:5000/app"
        ));
        assert!(!image_matches(id, "nginx", "1.27", "nginx"));
        assert!(!image_matches(id, "nginx", "latest", "redis"));
    }
}
