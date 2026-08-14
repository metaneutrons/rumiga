use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("firmware must be a direct workspace child")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn parse_toml(path: &Path) -> toml::Value {
    let table: toml::Table = toml::from_str(&read(path))
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
    toml::Value::Table(table)
}

fn manifest_string<'a>(manifest: &'a toml::Value, section: &str, key: &str) -> &'a str {
    manifest[section][key]
        .as_str()
        .unwrap_or_else(|| panic!("{section}.{key} must be a string"))
}

fn is_full_git_commit(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn locked_version<'a>(lockfile: &'a toml::Value, package_name: &str) -> &'a str {
    lockfile["package"]
        .as_array()
        .expect("Cargo.lock must contain packages")
        .iter()
        .find(|package| package["name"].as_str() == Some(package_name))
        .and_then(|package| package["version"].as_str())
        .unwrap_or_else(|| panic!("Cargo.lock must contain {package_name}"))
}

#[test]
fn pins_match_their_consuming_manifests() {
    let root = workspace_root();
    let manifest = parse_toml(&root.join("toolchain/manifest.toml"));
    let rust_toolchain = parse_toml(&root.join("rust-toolchain.toml"));
    let firmware_toolchain = parse_toml(&root.join("firmware/rust-toolchain.toml"));
    let cargo = parse_toml(&root.join("Cargo.toml"));
    let cargo_lock = parse_toml(&root.join("Cargo.lock"));
    let cargo_config = parse_toml(&root.join(".cargo/config.toml"));
    let package: serde_json::Value = serde_json::from_str(&read(&root.join("web/package.json")))
        .expect("web/package.json must be valid JSON");
    let package_lock: serde_json::Value =
        serde_json::from_str(&read(&root.join("web/package-lock.json")))
            .expect("web/package-lock.json must be valid JSON");

    let host_rust = manifest_string(&manifest, "host", "rust");
    let host_rust_msrv = manifest_string(&manifest, "host", "rust_msrv");
    let host_node = manifest_string(&manifest, "host", "node");
    let host_npm = manifest_string(&manifest, "host", "npm");
    let embedded_rust = manifest_string(&manifest, "embedded_rust", "channel");
    let esp_idf_reference = manifest_string(&manifest, "esp_idf", "git_reference");
    let esp_idf_repository = manifest_string(&manifest, "esp_idf", "repository");
    let esp_idf_svc = manifest_string(&manifest, "esp_rust_crates", "esp_idf_svc");

    assert_eq!(
        rust_toolchain["toolchain"]["channel"].as_str(),
        Some(host_rust)
    );
    assert_eq!(
        firmware_toolchain["toolchain"]["channel"].as_str(),
        Some(embedded_rust)
    );
    assert_eq!(
        cargo["workspace"]["package"]["rust-version"].as_str(),
        Some(host_rust_msrv)
    );
    assert_eq!(read(&root.join(".node-version")).trim(), host_node);
    assert_eq!(package["engines"]["node"].as_str(), Some(host_node));
    assert_eq!(package["engines"]["npm"].as_str(), Some(host_npm));
    assert_eq!(package_lock["packages"][""]["engines"], package["engines"]);
    assert_eq!(
        package["packageManager"].as_str(),
        Some(format!("npm@{host_npm}").as_str())
    );
    assert_eq!(
        cargo["workspace"]["dependencies"]["esp-idf-svc"]["version"].as_str(),
        Some(format!("={esp_idf_svc}").as_str())
    );
    assert_eq!(
        cargo_config["env"]["ESP_IDF_VERSION"]["value"].as_str(),
        Some(esp_idf_reference)
    );
    assert_eq!(
        cargo_config["env"]["ESP_IDF_REPOSITORY"]["value"].as_str(),
        Some(esp_idf_repository)
    );
    for key in ["ESP_IDF_SDKCONFIG", "ESP_IDF_SDKCONFIG_DEFAULTS"] {
        assert_eq!(
            cargo_config["env"][key]["relative"].as_bool(),
            Some(true),
            "{key} must resolve from the workspace Cargo config"
        );
    }
    assert_eq!(
        cargo_config["target"]["riscv32imafc-esp-espidf"]["linker"].as_str(),
        Some("ldproxy")
    );
    for (manifest_key, package_name) in [
        ("esp_idf_svc", "esp-idf-svc"),
        ("esp_idf_hal", "esp-idf-hal"),
        ("esp_idf_sys", "esp-idf-sys"),
        ("embuild", "embuild"),
    ] {
        assert_eq!(
            locked_version(&cargo_lock, package_name),
            manifest_string(&manifest, "esp_rust_crates", manifest_key)
        );
    }
}

#[test]
fn source_revisions_and_upgrade_state_are_explicit() {
    let root = workspace_root();
    let manifest = parse_toml(&root.join("toolchain/manifest.toml"));
    let idf_commit = manifest_string(&manifest, "esp_idf", "commit");
    let candidate_commit = manifest_string(&manifest, "esp_idf_upgrade_candidate", "commit");
    let bsp_commit = manifest_string(&manifest, "seeed_bsp", "commit");

    assert_eq!(manifest["schema_version"].as_integer(), Some(1));
    assert_eq!(manifest["status"].as_str(), Some("pinned-not-cross-built"));
    assert!(is_full_git_commit(idf_commit));
    assert!(is_full_git_commit(candidate_commit));
    assert!(is_full_git_commit(bsp_commit));
    assert_ne!(idf_commit, candidate_commit);
    assert_eq!(
        manifest_string(&manifest, "esp_idf", "git_reference"),
        format!("commit:{idf_commit}")
    );
    assert_eq!(
        manifest_string(&manifest, "esp_idf", "version"),
        manifest_string(&manifest, "seeed_bsp", "esp_idf_version")
    );
    assert_eq!(
        manifest["esp_idf_upgrade_candidate"]["status"].as_str(),
        Some("blocked")
    );
    assert_eq!(
        manifest["build"]["idf_path_policy"].as_str(),
        Some("must-be-unset")
    );
}
