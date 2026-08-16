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

fn locked_package<'a>(lockfile: &'a toml::Value, package_name: &str) -> &'a toml::Value {
    lockfile["package"]
        .as_array()
        .expect("Cargo.lock must contain packages")
        .iter()
        .find(|package| package["name"].as_str() == Some(package_name))
        .unwrap_or_else(|| panic!("Cargo.lock must contain {package_name}"))
}

fn locked_version<'a>(lockfile: &'a toml::Value, package_name: &str) -> &'a str {
    locked_package(lockfile, package_name)["version"]
        .as_str()
        .unwrap_or_else(|| panic!("Cargo.lock must contain {package_name}"))
}

fn string_array(value: &toml::Value) -> Vec<&str> {
    value
        .as_array()
        .expect("value must be an array")
        .iter()
        .map(|entry| entry.as_str().expect("array entries must be strings"))
        .collect()
}

fn assert_vellum_reuse_policy(root: &Path, manifest: &toml::Value, destination_license: &str) {
    let section = "d1001_idf6_evidence";
    for (key, expected) in [
        ("usage", "authorized-source-reuse"),
        ("source_license", "AGPL-3.0-or-later"),
        (
            "reuse_authorization",
            "copyright-holder-authorized-for-rumiga-gpl-3.0-only",
        ),
        ("third_party_policy", "separate-license-review-required"),
    ] {
        assert_eq!(manifest_string(manifest, section, key), expected);
    }
    assert_eq!(
        manifest_string(manifest, section, "destination_license"),
        destination_license
    );

    let provenance_policy = manifest_string(manifest, section, "provenance_policy");
    let provenance = read(&root.join(provenance_policy));
    for required_record in [
        manifest_string(manifest, section, "repository"),
        manifest_string(manifest, section, "revision"),
        destination_license,
        "third-party",
        "Transfer Register",
    ] {
        assert!(
            provenance.contains(required_record),
            "{provenance_policy} must record {required_record}"
        );
    }
}

fn assert_ci_tool_pins(root: &Path, manifest: &toml::Value) {
    let ci_workflow = read(&root.join(".github/workflows/ci.yml"));
    let host_job = ci_workflow
        .split_once("  host:\n")
        .and_then(|(_, remainder)| remainder.split_once("\n  compatibility:\n"))
        .map(|(host_job, _)| host_job)
        .expect("CI workflow must contain a bounded host job");
    let host_rust = manifest_string(manifest, "host", "rust");
    let host_rust_msrv = manifest_string(manifest, "host", "rust_msrv");
    let cargo_audit = manifest_string(manifest, "tools", "cargo_audit");
    let cargo_deny = manifest_string(manifest, "tools", "cargo_deny");
    let embedded_rust = manifest_string(manifest, "embedded_rust", "channel");
    let espflash = manifest_string(manifest, "tools", "espflash");
    let ldproxy = manifest_string(manifest, "tools", "ldproxy");

    assert!(ci_workflow.contains(&format!("CARGO_AUDIT_VERSION: \"{cargo_audit}\"")));
    assert!(ci_workflow.contains(&format!("CARGO_DENY_VERSION: \"{cargo_deny}\"")));
    assert!(ci_workflow.contains(&format!("EMBEDDED_RUST_CHANNEL: \"{embedded_rust}\"")));
    assert!(ci_workflow.contains(&format!("ESPFLASH_VERSION: \"{espflash}\"")));
    assert!(ci_workflow.contains(&format!("LDPROXY_VERSION: \"{ldproxy}\"")));
    for gate in ["lockfiles", "host", "supply-chain", "portable", "firmware"] {
        assert!(
            ci_workflow.contains(&format!("cargo +{host_rust} xtask ci --gate {gate}")),
            "CI must invoke the canonical {gate} gate"
        );
    }
    assert!(
        host_job.contains(&format!(
            "rustup toolchain install {host_rust_msrv} --profile minimal --no-self-update"
        )),
        "CI host job must install the declared Rust MSRV"
    );
    assert!(
        ci_workflow
            .contains("actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1")
    );
}

fn assert_target_baseline(manifest: &toml::Value) {
    assert_eq!(manifest["target"]["physical_flash"].as_str(), Some("32MB"));
    assert_eq!(
        manifest["target"]["configured_flash"].as_str(),
        Some("16MB")
    );
    assert_eq!(manifest["target"]["physical_psram"].as_str(), Some("32MB"));
    assert_eq!(
        manifest["portable_rust"]["scope"].as_str(),
        Some("portable-core-boundary")
    );
    let portable_profiles = manifest["portable_rust"]["profiles"]
        .as_array()
        .expect("portable_rust.profiles must be an array");
    assert_eq!(portable_profiles.len(), 2);

    let foundation = portable_profiles
        .iter()
        .find(|profile| profile["name"].as_str() == Some("foundation"))
        .expect("foundation portable profile must exist");
    assert_eq!(
        string_array(&foundation["packages"]),
        ["m68000", "rumiga-api", "rumiga-platform"]
    );
    assert_eq!(foundation["release"].as_bool(), Some(false));
    assert_eq!(foundation["default_features"].as_bool(), Some(true));
    assert!(string_array(&foundation["features"]).is_empty());

    let stock_core = portable_profiles
        .iter()
        .find(|profile| profile["name"].as_str() == Some("stock-amiga-core"))
        .expect("stock-amiga-core portable profile must exist");
    assert_eq!(
        string_array(&stock_core["packages"]),
        ["m68k", "rumiga-core"]
    );
    assert_eq!(stock_core["release"].as_bool(), Some(true));
    assert_eq!(stock_core["default_features"].as_bool(), Some(false));
    assert_eq!(string_array(&stock_core["features"]), ["no_std"]);
    assert_eq!(
        manifest["build"]["evidence_schema"].as_str(),
        Some("rumiga.firmware.build.v1")
    );
}

#[test]
fn pins_match_their_consuming_manifests() {
    let root = workspace_root();
    let manifest = parse_toml(&root.join("toolchain/manifest.toml"));
    let rust_toolchain = parse_toml(&root.join("rust-toolchain.toml"));
    let firmware_toolchain = parse_toml(&root.join("firmware/rust-toolchain.toml"));
    let cargo = parse_toml(&root.join("Cargo.toml"));
    let firmware_cargo = parse_toml(&root.join("firmware/Cargo.toml"));
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
    let embuild = manifest_string(&manifest, "esp_rust_crates", "embuild");

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
        cargo["workspace"]["dependencies"]["embuild"].as_str(),
        Some(format!("={embuild}").as_str())
    );
    assert_eq!(
        firmware_cargo["build-dependencies"]["embuild"]["workspace"].as_bool(),
        Some(true)
    );
    assert_eq!(
        string_array(&firmware_cargo["build-dependencies"]["embuild"]["features"]),
        ["espidf"]
    );
    assert!(read(&root.join("firmware/build.rs")).contains("embuild::espidf::sysenv::output();"));
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
    assert_eq!(
        cargo_config["env"]["ESP_IDF_SYS_ROOT_CRATE"]["value"].as_str(),
        manifest["target"]["cargo_package"].as_str()
    );
    assert_target_baseline(&manifest);
    assert_ci_tool_pins(&root, &manifest);
    assert_eq!(
        string_array(&cargo_config["unstable"]["build-std"]),
        string_array(&manifest["embedded_rust"]["build_std"])
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
    let cargo = parse_toml(&root.join("Cargo.toml"));
    let cargo_lock = parse_toml(&root.join("Cargo.lock"));
    let idf_commit = manifest_string(&manifest, "esp_idf", "commit");
    let candidate_commit = manifest_string(&manifest, "esp_idf_patch_candidate", "commit");
    let bsp_commit = manifest_string(&manifest, "seeed_bsp", "commit");
    let evidence_commit = manifest_string(&manifest, "d1001_idf6_evidence", "revision");
    let evidence_idf_commit = manifest_string(&manifest, "d1001_idf6_evidence", "idf_commit");

    assert_eq!(manifest["schema_version"].as_integer(), Some(1));
    assert_eq!(manifest["status"].as_str(), Some("cross-built-not-hil"));
    assert!(is_full_git_commit(idf_commit));
    assert!(is_full_git_commit(candidate_commit));
    assert!(is_full_git_commit(bsp_commit));
    assert!(is_full_git_commit(evidence_commit));
    assert!(is_full_git_commit(evidence_idf_commit));
    assert_ne!(idf_commit, candidate_commit);
    assert_eq!(idf_commit, evidence_idf_commit);
    assert_eq!(
        manifest_string(&manifest, "esp_idf", "git_reference"),
        "tag:v6.0"
    );
    assert_eq!(manifest_string(&manifest, "esp_idf", "version"), "6.0.0");
    assert_eq!(
        manifest_string(&manifest, "d1001_idf6_evidence", "idf_version"),
        manifest_string(&manifest, "esp_idf", "version")
    );
    assert_eq!(
        manifest["esp_idf_patch_candidate"]["status"].as_str(),
        Some("awaiting-upstream-rust-compatibility")
    );
    assert_eq!(
        manifest_string(&manifest, "esp_idf_patch_candidate", "git_reference"),
        "tag:v6.0.2"
    );
    assert_eq!(
        manifest_string(&manifest, "seeed_bsp", "role"),
        "hardware-reference-only"
    );
    assert_ne!(
        manifest_string(&manifest, "esp_idf", "version"),
        manifest_string(&manifest, "seeed_bsp", "esp_idf_version")
    );
    let workspace_license = cargo["workspace"]["package"]["license"]
        .as_str()
        .expect("workspace license must be a string");
    assert_eq!(workspace_license, "GPL-3.0-only");
    assert_vellum_reuse_policy(&root, &manifest, workspace_license);
    assert_eq!(
        manifest["build"]["idf_path_policy"].as_str(),
        Some("must-be-unset")
    );
    assert_eq!(
        manifest["build"]["idf_reference_policy"].as_str(),
        Some("tag-with-verified-commit")
    );

    for (manifest_prefix, package_name) in [
        ("esp_idf_svc", "esp-idf-svc"),
        ("esp_idf_hal", "esp-idf-hal"),
        ("esp_idf_sys", "esp-idf-sys"),
    ] {
        let repository_key = format!("{manifest_prefix}_repository");
        let commit_key = format!("{manifest_prefix}_commit");
        let repository = manifest_string(&manifest, "esp_rust_patches", &repository_key);
        let commit = manifest_string(&manifest, "esp_rust_patches", &commit_key);
        let patch = &cargo["patch"]["crates-io"][package_name];
        let source = locked_package(&cargo_lock, package_name)["source"]
            .as_str()
            .unwrap_or_else(|| panic!("{package_name} must have a locked Git source"));

        assert!(is_full_git_commit(commit));
        assert_eq!(patch["git"].as_str(), Some(repository));
        assert_eq!(patch["rev"].as_str(), Some(commit));
        assert_eq!(source, format!("git+{repository}?rev={commit}#{commit}"));
    }
}
