//! Checks a published GitHub release with the updater's own code, as an installed app would
//! read it. The release workflow runs it after publishing (`cargo test --test
//! published_release -- --ignored`), with:
//!
//! - `COLIMA_GPUI_RELEASE_JSON`: a file holding `GET /repos/<repo>/releases/latest`
//! - `COLIMA_GPUI_RELEASE_SUMS`: the release's downloaded `SHA256SUMS`
//! - `COLIMA_GPUI_RELEASE_VERSION`: the version just released, e.g. `0.2.0`

use colima_gpui::update::{Release, Version, checksum};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore = "run by the release workflow against the release it just published"]
fn published_release_is_one_the_updater_can_install() {
    let json = std::fs::read_to_string(env("COLIMA_GPUI_RELEASE_JSON")).unwrap();
    let release: Release = serde_json::from_str(&json).expect("the updater can parse the release");
    let version = env("COLIMA_GPUI_RELEASE_VERSION");

    assert_eq!(
        release.version(),
        Version::parse(&version),
        "releases/latest is {}, not the version just published",
        release.tag_name
    );

    let sums = std::fs::read_to_string(env("COLIMA_GPUI_RELEASE_SUMS")).unwrap();
    assert!(release.assets.iter().any(|a| a.name == "SHA256SUMS"));
    for (platform, arch) in [
        ("macos", "aarch64"),
        ("macos", "x86_64"),
        ("linux", "aarch64"),
        ("linux", "x86_64"),
    ] {
        let asset = release
            .asset_for_platform(platform, arch)
            .unwrap_or_else(|| panic!("no update for {arch}"));
        assert!(asset.size > 0, "{} is empty", asset.name);
        checksum(&sums, &asset.name).unwrap_or_else(|e| panic!("{}: {e}", asset.name));
    }
}
