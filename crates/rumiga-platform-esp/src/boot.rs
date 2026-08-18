// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Boot policy mirror and the manifest a device emits at boot.
//!
//! # Why the policy is mirrored here
//!
//! `toolchain/manifest.toml` `[boot_policy]` is where these values are declared, and the
//! firmware gate compares that table against the resolved `sdkconfig`. The firmware cannot
//! read the same values from the build: `esp-idf-sys` exposes boolean and choice options as
//! cargo cfgs, but integer options such as `CONFIG_ESP_TASK_WDT_TIMEOUT_S` reach Rust as
//! nothing at all. So the numbers are written twice, and
//! [`the mirror test`](../../tests/boot_policy.rs) pins this mirror against the manifest so
//! the duplication cannot drift unnoticed.
//!
//! # What the manifest reports
//!
//! Two kinds of value, kept apart on purpose. The policy is what the image was configured
//! with. The observations are what the running system actually has, read at boot. A device
//! whose observed PSRAM disagrees with the configured budget is the case worth seeing, and
//! echoing configuration back would hide exactly that.

use core::fmt::Write as _;

/// Schema of the boot manifest text.
pub const BOOT_MANIFEST_SCHEMA: &str = "rumiga.boot.v1";

/// PSRAM allocator policy, mirroring `[boot_policy.psram]`.
pub const PSRAM_ALLOCATOR: &str = "malloc";
/// Allocations at or below this size come from internal RAM.
pub const PSRAM_ALWAYS_INTERNAL_BYTES: u32 = 8192;
/// Internal RAM kept available once PSRAM joins the general heap.
pub const PSRAM_RESERVE_INTERNAL_BYTES: u32 = 65536;

/// Panic policy, mirroring `[boot_policy.panic]`.
pub const PANIC_ACTION: &str = "print-reboot";
/// Delay before the reboot a panic causes.
pub const PANIC_REBOOT_DELAY_SECONDS: u32 = 0;

/// Core-dump policy, mirroring `[boot_policy.core_dump]`.
pub const CORE_DUMP_TARGET: &str = "flash";
/// Whole-DRAM capture, off because the reserved partition is smaller than ESP-IDF requires.
pub const CORE_DUMP_CAPTURES_DRAM: bool = false;
/// Whether a stored dump is checksum checked at boot.
pub const CORE_DUMP_CHECKED_ON_BOOT: bool = true;
/// Upper bound on tasks recorded in a dump.
pub const CORE_DUMP_MAX_TASKS: u32 = 64;

/// Task watchdog period, mirroring `[boot_policy.watchdog]`.
pub const TASK_WATCHDOG_TIMEOUT_SECONDS: u32 = 5;
/// Whether a task watchdog timeout panics, and therefore reboots.
pub const TASK_WATCHDOG_PANICS: bool = true;
/// Whether the CPU0 idle task is subscribed to the task watchdog.
pub const TASK_WATCHDOG_CHECKS_IDLE_CPU0: bool = true;
/// Whether the CPU1 idle task is subscribed to the task watchdog.
pub const TASK_WATCHDOG_CHECKS_IDLE_CPU1: bool = true;
/// Interrupt watchdog period.
pub const INTERRUPT_WATCHDOG_TIMEOUT_MILLIS: u32 = 300;
/// Bootloader watchdog period.
pub const BOOTLOADER_WATCHDOG_TIMEOUT_MILLIS: u32 = 9000;

/// Default log level, mirroring `[boot_policy.logging]`.
pub const LOG_DEFAULT_LEVEL: u32 = 3;
/// Highest level compiled in, above which call sites are removed.
pub const LOG_MAXIMUM_LEVEL: u32 = 4;

/// Why the device started.
///
/// The variants follow `esp_reset_reason_t`. The distinction that matters for this product
/// is [`Self::TaskWatchdog`] against [`Self::Panic`]: the boot policy makes a task watchdog
/// timeout panic, so a device that reboots because the frame loop stopped yielding looks
/// like a watchdog reset, not like a crash, and the two must not be conflated in a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetReason {
    /// Power was applied.
    PowerOn,
    /// The external reset pin was asserted.
    ExternalPin,
    /// `esp_restart` was called.
    Software,
    /// A panic or unhandled exception.
    Panic,
    /// The interrupt watchdog fired.
    InterruptWatchdog,
    /// The task watchdog fired.
    TaskWatchdog,
    /// Another watchdog fired.
    OtherWatchdog,
    /// Deep sleep ended.
    DeepSleep,
    /// The supply dropped below the brownout threshold.
    Brownout,
    /// A reset arrived over SDIO.
    Sdio,
    /// The USB peripheral reset the chip.
    UsbPeripheral,
    /// JTAG reset the chip.
    Jtag,
    /// An eFuse error reset the chip.
    EfuseError,
    /// Power glitch detection reset the chip.
    PowerGlitch,
    /// CPU lockup detection reset the chip.
    CpuLockup,
    /// The reason could not be determined.
    Unknown,
}

impl ResetReason {
    /// Stable name for the manifest.
    ///
    /// Kept separate from `Debug` so a rename of a variant cannot silently change recorded
    /// evidence.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PowerOn => "power-on",
            Self::ExternalPin => "external-pin",
            Self::Software => "software",
            Self::Panic => "panic",
            Self::InterruptWatchdog => "interrupt-watchdog",
            Self::TaskWatchdog => "task-watchdog",
            Self::OtherWatchdog => "other-watchdog",
            Self::DeepSleep => "deep-sleep",
            Self::Brownout => "brownout",
            Self::Sdio => "sdio",
            Self::UsbPeripheral => "usb-peripheral",
            Self::Jtag => "jtag",
            Self::EfuseError => "efuse-error",
            Self::PowerGlitch => "power-glitch",
            Self::CpuLockup => "cpu-lockup",
            Self::Unknown => "unknown",
        }
    }

    /// Whether this reason means the previous run ended badly.
    ///
    /// A supervisor counts these; the others are ordinary starts. Brownout is included
    /// because it is a power fault, not a clean start.
    #[must_use]
    pub const fn is_fault(self) -> bool {
        matches!(
            self,
            Self::Panic
                | Self::InterruptWatchdog
                | Self::TaskWatchdog
                | Self::OtherWatchdog
                | Self::Brownout
                | Self::EfuseError
                | Self::PowerGlitch
                | Self::CpuLockup
        )
    }
}

/// What the running system actually has, as opposed to what it was configured with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BootObservations {
    /// Total PSRAM the `SoC` reports.
    pub psram_total_bytes: u32,
    /// PSRAM still free when the manifest was taken.
    pub psram_free_bytes: u32,
    /// Internal RAM still free when the manifest was taken.
    pub internal_free_bytes: u32,
}

/// The report a device emits at boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootManifest {
    /// Why the device started.
    pub reset_reason: ResetReason,
    /// What the running system has.
    pub observations: BootObservations,
}

impl BootManifest {
    /// Build a manifest from a reset reason and the observations taken beside it.
    #[must_use]
    pub const fn new(reset_reason: ResetReason, observations: BootObservations) -> Self {
        Self {
            reset_reason,
            observations,
        }
    }

    /// Render the manifest as one `key=value` pair per line.
    ///
    /// Text rather than JSON, because the consumer is the serial console and a line-oriented
    /// form can be read by a person and diffed between boots without a parser. The order is
    /// fixed so two boots can be compared directly, which is why the three sections are
    /// written in sequence rather than assembled from a map.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "schema={BOOT_MANIFEST_SCHEMA}");
        self.write_reset(&mut text);
        Self::write_policy(&mut text);
        self.write_observations(&mut text);
        text
    }

    /// Why the device started, and whether that counts as a fault.
    fn write_reset(&self, text: &mut String) {
        let _ = writeln!(text, "reset_reason={}", self.reset_reason.as_str());
        let _ = writeln!(text, "reset_is_fault={}", self.reset_reason.is_fault());
    }

    /// What the image was configured with.
    ///
    /// Associated rather than taking `self`, because none of it depends on the boot: these
    /// are the mirrored constants, and a reader should not have to check whether a value
    /// came from the running system.
    fn write_policy(text: &mut String) {
        let _ = writeln!(text, "psram_allocator={PSRAM_ALLOCATOR}");
        let _ = writeln!(
            text,
            "psram_always_internal_bytes={PSRAM_ALWAYS_INTERNAL_BYTES}"
        );
        let _ = writeln!(
            text,
            "psram_reserve_internal_bytes={PSRAM_RESERVE_INTERNAL_BYTES}"
        );
        let _ = writeln!(text, "panic_action={PANIC_ACTION}");
        let _ = writeln!(
            text,
            "panic_reboot_delay_seconds={PANIC_REBOOT_DELAY_SECONDS}"
        );
        let _ = writeln!(text, "core_dump_target={CORE_DUMP_TARGET}");
        let _ = writeln!(text, "core_dump_captures_dram={CORE_DUMP_CAPTURES_DRAM}");
        let _ = writeln!(
            text,
            "core_dump_checked_on_boot={CORE_DUMP_CHECKED_ON_BOOT}"
        );
        let _ = writeln!(text, "core_dump_max_tasks={CORE_DUMP_MAX_TASKS}");
        let _ = writeln!(
            text,
            "task_watchdog_timeout_seconds={TASK_WATCHDOG_TIMEOUT_SECONDS}"
        );
        let _ = writeln!(text, "task_watchdog_panics={TASK_WATCHDOG_PANICS}");
        let _ = writeln!(
            text,
            "task_watchdog_checks_idle_cpu0={TASK_WATCHDOG_CHECKS_IDLE_CPU0}"
        );
        let _ = writeln!(
            text,
            "task_watchdog_checks_idle_cpu1={TASK_WATCHDOG_CHECKS_IDLE_CPU1}"
        );
        let _ = writeln!(
            text,
            "interrupt_watchdog_timeout_millis={INTERRUPT_WATCHDOG_TIMEOUT_MILLIS}"
        );
        let _ = writeln!(
            text,
            "bootloader_watchdog_timeout_millis={BOOTLOADER_WATCHDOG_TIMEOUT_MILLIS}"
        );
        let _ = writeln!(text, "log_default_level={LOG_DEFAULT_LEVEL}");
        let _ = writeln!(text, "log_maximum_level={LOG_MAXIMUM_LEVEL}");
    }

    /// What the running system actually has.
    fn write_observations(&self, text: &mut String) {
        let _ = writeln!(
            text,
            "psram_total_bytes={}",
            self.observations.psram_total_bytes
        );
        let _ = writeln!(
            text,
            "psram_free_bytes={}",
            self.observations.psram_free_bytes
        );
        let _ = writeln!(
            text,
            "internal_free_bytes={}",
            self.observations.internal_free_bytes
        );
    }
}

/// Read the reset reason from ESP-IDF.
///
/// `esp_idf_svc::hal::reset::ResetReason::get` is a safe wrapper, which matters because the
/// workspace forbids unsafe code and the underlying `esp_reset_reason` is an FFI call.
///
/// The variants ESP-IDF gained after 5.1 are behind its own version cfgs, so the match falls
/// through to [`ResetReason::Unknown`] rather than naming them and failing to compile on a
/// different pinned ESP-IDF.
#[cfg(target_os = "espidf")]
#[must_use]
pub fn reset_reason() -> ResetReason {
    use esp_idf_svc::hal::reset::ResetReason as Idf;

    match Idf::get() {
        Idf::PowerOn => ResetReason::PowerOn,
        Idf::ExternalPin => ResetReason::ExternalPin,
        Idf::Software => ResetReason::Software,
        Idf::Panic => ResetReason::Panic,
        Idf::InterruptWatchdog => ResetReason::InterruptWatchdog,
        Idf::TaskWatchdog => ResetReason::TaskWatchdog,
        Idf::Watchdog => ResetReason::OtherWatchdog,
        Idf::DeepSleep => ResetReason::DeepSleep,
        Idf::Brownout => ResetReason::Brownout,
        Idf::Sdio => ResetReason::Sdio,
        _ => ResetReason::Unknown,
    }
}
