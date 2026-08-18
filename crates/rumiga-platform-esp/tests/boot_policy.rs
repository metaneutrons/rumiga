// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Pin the boot policy mirror against the declaration it mirrors.
//!
//! The numbers in `boot::` are written twice, here and in `toolchain/manifest.toml`, because
//! integer `sdkconfig` values do not reach Rust as cfgs and the firmware cannot read them
//! from the build. Duplication that nothing checks drifts, and a boot manifest reporting a
//! watchdog period the device does not run would be worse than reporting nothing. This test
//! is the other half of the firmware gate's comparison: the gate pins the declaration
//! against the resolved `sdkconfig`, and this pins the mirror against the declaration.

use std::fs;
use std::path::{Path, PathBuf};

use rumiga_platform_esp::boot;

fn manifest() -> toml::Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate sits two levels below the workspace root")
        .to_path_buf();
    let path: PathBuf = root.join("toolchain/manifest.toml");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let table: toml::Table = toml::from_str(&text)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
    toml::Value::Table(table)
}

fn integer(policy: &toml::Value, section: &str, key: &str) -> u32 {
    let value = policy[section][key]
        .as_integer()
        .unwrap_or_else(|| panic!("boot_policy.{section}.{key} must be an integer"));
    u32::try_from(value)
        .unwrap_or_else(|_| panic!("boot_policy.{section}.{key} does not fit in u32"))
}

fn boolean(policy: &toml::Value, section: &str, key: &str) -> bool {
    policy[section][key]
        .as_bool()
        .unwrap_or_else(|| panic!("boot_policy.{section}.{key} must be a boolean"))
}

fn text<'a>(policy: &'a toml::Value, section: &str, key: &str) -> &'a str {
    policy[section][key]
        .as_str()
        .unwrap_or_else(|| panic!("boot_policy.{section}.{key} must be a string"))
}

/// The mirrored strings match the declaration.
///
/// Split by value kind so a failure names which kind drifted, and so each table stays short
/// enough to read at a glance.
#[test]
fn the_mirrored_strings_match_the_declaration() {
    let manifest = manifest();
    let policy = &manifest["boot_policy"];

    for (section, key, mirrored) in [
        ("psram", "allocator", boot::PSRAM_ALLOCATOR),
        ("panic", "action", boot::PANIC_ACTION),
        ("core_dump", "target", boot::CORE_DUMP_TARGET),
    ] {
        assert_eq!(text(policy, section, key), mirrored, "{section}.{key}");
    }
}

/// The mirrored integers match the declaration.
///
/// These are the values that cannot reach Rust from the build at all, so this is the table
/// the duplication exists for.
#[test]
fn the_mirrored_integers_match_the_declaration() {
    let manifest = manifest();
    let policy = &manifest["boot_policy"];

    for (section, key, mirrored) in [
        (
            "psram",
            "always_internal_bytes",
            boot::PSRAM_ALWAYS_INTERNAL_BYTES,
        ),
        (
            "psram",
            "reserve_internal_bytes",
            boot::PSRAM_RESERVE_INTERNAL_BYTES,
        ),
        (
            "panic",
            "reboot_delay_seconds",
            boot::PANIC_REBOOT_DELAY_SECONDS,
        ),
        ("core_dump", "max_tasks", boot::CORE_DUMP_MAX_TASKS),
        (
            "watchdog",
            "task_timeout_seconds",
            boot::TASK_WATCHDOG_TIMEOUT_SECONDS,
        ),
        (
            "watchdog",
            "interrupt_timeout_millis",
            boot::INTERRUPT_WATCHDOG_TIMEOUT_MILLIS,
        ),
        (
            "watchdog",
            "bootloader_timeout_millis",
            boot::BOOTLOADER_WATCHDOG_TIMEOUT_MILLIS,
        ),
        ("logging", "default_level", boot::LOG_DEFAULT_LEVEL),
        ("logging", "maximum_level", boot::LOG_MAXIMUM_LEVEL),
    ] {
        assert_eq!(integer(policy, section, key), mirrored, "{section}.{key}");
    }
}

/// The mirrored booleans match the declaration.
#[test]
fn the_mirrored_booleans_match_the_declaration() {
    let manifest = manifest();
    let policy = &manifest["boot_policy"];

    for (section, key, mirrored) in [
        ("core_dump", "captures_dram", boot::CORE_DUMP_CAPTURES_DRAM),
        (
            "core_dump",
            "checked_on_boot",
            boot::CORE_DUMP_CHECKED_ON_BOOT,
        ),
        ("watchdog", "task_panics", boot::TASK_WATCHDOG_PANICS),
        (
            "watchdog",
            "task_checks_idle_cpu0",
            boot::TASK_WATCHDOG_CHECKS_IDLE_CPU0,
        ),
        (
            "watchdog",
            "task_checks_idle_cpu1",
            boot::TASK_WATCHDOG_CHECKS_IDLE_CPU1,
        ),
    ] {
        assert_eq!(boolean(policy, section, key), mirrored, "{section}.{key}");
    }
}

/// The declaration must not gain a value the mirror does not carry.
///
/// The table-driven test above checks every row it lists; this checks that the rows cover
/// the declaration, which is the direction that fails silently.
#[test]
fn the_mirror_covers_every_declared_value() {
    let manifest = manifest();
    let policy = manifest["boot_policy"]
        .as_table()
        .expect("boot_policy must be a table");

    let declared: usize = policy
        .values()
        .map(|section| {
            section
                .as_table()
                .expect("every boot_policy section must be a table")
                .len()
        })
        .sum();

    // Seventeen values across five sections: three PSRAM, two panic, four core dump, six
    // watchdog, two logging. Adding one without a mirrored constant and a row above fails
    // here, which is the point.
    assert_eq!(
        declared, 17,
        "the declared boot policy has {declared} values; add the mirror and its row, then \
         update this count"
    );
    assert_eq!(policy.len(), 5, "boot_policy must have five sections");
}

/// A watchdog reset must not read as a crash.
///
/// The boot policy makes a task watchdog timeout panic, so the two arrive by the same route
/// and a report that conflated them would blame the wrong thing.
#[test]
fn a_watchdog_reset_is_distinguishable_from_a_panic() {
    assert_ne!(
        boot::ResetReason::TaskWatchdog.as_str(),
        boot::ResetReason::Panic.as_str()
    );
    assert!(boot::ResetReason::TaskWatchdog.is_fault());
    assert!(boot::ResetReason::Panic.is_fault());
}

#[test]
fn an_ordinary_start_is_not_a_fault() {
    for reason in [
        boot::ResetReason::PowerOn,
        boot::ResetReason::ExternalPin,
        boot::ResetReason::Software,
        boot::ResetReason::DeepSleep,
        boot::ResetReason::Sdio,
        boot::ResetReason::UsbPeripheral,
        boot::ResetReason::Jtag,
        boot::ResetReason::Unknown,
    ] {
        assert!(
            !reason.is_fault(),
            "{} must not count as a fault",
            reason.as_str()
        );
    }
}

/// Every reason renders to a distinct, stable name.
#[test]
fn reset_reason_names_are_distinct() {
    let names = [
        boot::ResetReason::PowerOn,
        boot::ResetReason::ExternalPin,
        boot::ResetReason::Software,
        boot::ResetReason::Panic,
        boot::ResetReason::InterruptWatchdog,
        boot::ResetReason::TaskWatchdog,
        boot::ResetReason::OtherWatchdog,
        boot::ResetReason::DeepSleep,
        boot::ResetReason::Brownout,
        boot::ResetReason::Sdio,
        boot::ResetReason::UsbPeripheral,
        boot::ResetReason::Jtag,
        boot::ResetReason::EfuseError,
        boot::ResetReason::PowerGlitch,
        boot::ResetReason::CpuLockup,
        boot::ResetReason::Unknown,
    ]
    .map(boot::ResetReason::as_str);

    let mut sorted = names.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    sorted.dedup();
    assert_eq!(sorted.len(), count, "reset reason names must be distinct");
}

/// The rendering reports the observations separately from the policy.
#[test]
fn the_manifest_reports_policy_and_observation() {
    let manifest = boot::BootManifest::new(
        boot::ResetReason::TaskWatchdog,
        boot::BootObservations {
            psram_total_bytes: 33_554_432,
            psram_free_bytes: 31_000_000,
            internal_free_bytes: 220_000,
        },
    );
    let text = manifest.to_text();

    assert!(text.starts_with("schema=rumiga.boot.v1\n"));
    assert!(text.contains("reset_reason=task-watchdog\n"));
    assert!(text.contains("reset_is_fault=true\n"));
    // The configured threshold and the observed total are both present, and differ, which is
    // the point of reporting them separately.
    assert!(text.contains("psram_always_internal_bytes=8192\n"));
    assert!(text.contains("psram_total_bytes=33554432\n"));
    assert!(text.contains("psram_free_bytes=31000000\n"));
    assert!(text.contains("internal_free_bytes=220000\n"));
    assert!(text.ends_with('\n'));
}

/// Two boots differing only in the reset reason render differently.
#[test]
fn the_rendering_is_sensitive_to_the_reset_reason() {
    let observations = boot::BootObservations::default();
    let first = boot::BootManifest::new(boot::ResetReason::PowerOn, observations).to_text();
    let second = boot::BootManifest::new(boot::ResetReason::Panic, observations).to_text();
    assert_ne!(first, second);
}
