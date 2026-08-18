# ADR-0021: Declared Boot Policy

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M2-003

## Context

M2-003 asks for a PSRAM allocator, panic, watchdog, logging, and reset policy, with a boot
manifest reporting the values and the reset reason. Before this task none of it was
declared. `firmware/sdkconfig.defaults` set three PSRAM keys and said nothing about panic,
watchdogs, logging, or core dumps, so the policy was whatever ESP-IDF defaults to, unstated
and unchecked.

Reading the resolved configuration against the flash layout found three problems that had
nothing to do with writing a manifest.

M2-013 reserved a 108 KiB `coredump` partition and `CONFIG_ESP_COREDUMP_ENABLE_TO_NONE`
was set, so nothing would ever write to it. The layout permanently spent flash on a
diagnostic that was switched off, and a panic on a shipped device would leave nothing
behind.

The task watchdog did not reset. `CONFIG_ESP_TASK_WDT_PANIC` defaults off, so a hung task
logs a warning and the device stays hung.

The layout comparison did not compare the `encrypted` flag. `parse_partition_layout`
required five CSV fields and ignored the sixth, and `decode_partition_table` never read the
flags word at byte 28. So the check that claims the merged image carries the declared layout
entry by entry skipped the one field the security posture rests on: `nvs_keys` holds the NVS
encryption keys.

## Decision

Declare the policy in `toolchain/manifest.toml` `[boot_policy]` and check it from both
sides. The firmware gate compares the declaration against the resolved `sdkconfig`, so a
`sdkconfig.defaults` edit that does not take effect fails. `rumiga-platform-esp` mirrors the
values for the boot manifest and a host test pins the mirror against the declaration.

The mirror exists because it has to. `esp-idf-sys` exposes boolean and choice options as
cargo cfgs, but integer options such as `CONFIG_ESP_TASK_WDT_TIMEOUT_S` reach Rust as
nothing at all, so the firmware cannot read them from the build it is part of. What can be
avoided is the drift, and that is what the pinning test is for.

Write core dumps to flash, and mark the partition `encrypted`. ESP-IDF writes the dump in
plain text and only logs a warning when flash encryption is on and the flag is absent, and a
dump contains task stacks.

Leave whole-DRAM capture off. ESP-IDF states that enabling it needs at least 128 KiB
reserved and the partition is 108 KiB. The gate encodes the figure, so a future edit that
enables capture fails there instead of overflowing the partition on a device.

Make the task watchdog panic, and therefore reboot, on timeout, and keep both idle tasks
subscribed. A hung device that recovers itself beats one that stays hung.

This carries an obligation. The emulator frame loop will run hot, and a task that
monopolizes a core without yielding starves that core's idle task and trips the watchdog,
which now reboots. The loop must yield or subscribe itself and feed the watchdog. It must
not be given a pass by unsubscribing an idle task, which would trade a real check for a
convenience and leave a genuinely hung core undetected.

Compile DEBUG logging in and default to INFO, so field diagnosis can be raised at runtime
without a new image. Leave VERBOSE out.

Compare the `encrypted` flag in both directions, and reject an unknown flag rather than
dropping it.

Report the policy and the observations separately in the boot manifest. The policy is what
the image was configured with; the observations are what the running system has. A device
whose observed PSRAM disagrees with the configured budget is the case worth seeing, and
echoing configuration back would hide it.

## Consequences

The application grew from 182,512 to 204,400 bytes, 21,888 bytes for the core-dump
component and the DEBUG call sites. That is 3.25% of a 6 MiB slot.

A panic on a device now leaves a checksummed core dump in encrypted flash, holding task
stacks for up to 64 tasks.

The watchdog obligation is recorded rather than solved. Nothing enforces it today, because
the frame loop does not exist; M2-004 and the loop that follows inherit it.

Whole-DRAM capture is out for the life of this layout. The partition is 108 KiB against a
documented 128 KiB minimum, and application slot offsets are effectively permanent once
devices ship.

The reset reason is absent from the firmware evidence bundle, recorded as `null` rather than
a placeholder. It is a runtime value and no board has booted this image. The bundle claims
`declared-boot-policy-verified`, which says the image runs the declared policy, and excludes
`boot-manifest-not-emitted`, which is the half that awaits hardware. Keeping those apart is
the point: a single claim covering both would overstate what a build can show.

The boot manifest's text form is not emitted by anything yet. `firmware/src/main.rs` is
still a stub, so the manifest is a type with a rendering and a reader, compiled but never
run on a device.

## Alternatives

Leaving the ESP-IDF defaults in place was rejected. They are reasonable defaults for a
development board and two of them are wrong for an appliance, and an undeclared policy
cannot be checked against anything.

Enabling whole-DRAM capture was rejected on the partition size, not on preference. The
alternative was enlarging the partition, which moves the application slots, which an OTA
image cannot survive on a device already flashed with the old table.

Unsubscribing the emulator core's idle task was rejected. It removes the trade-off by
removing the check, and a genuinely hung core would then be visible only through a task that
had explicitly subscribed.

Generating the mirror from the resolved `sdkconfig` in a build script was considered and
deferred. It would remove the duplication, and it makes the host build depend on a
configuration that only exists after an ESP-IDF build, which would make the host tests
unrunnable without the firmware toolchain.

Rendering the manifest as JSON was rejected. The consumer is the serial console, where a
line-oriented form can be read by a person and diffed between boots without a parser.

## Evidence

`CONFIG_ESP_COREDUMP_ENABLE_TO_NONE=y` was read from the resolved `sdkconfig` at line 2509
of the bundle preceding this change, alongside the 0x1B000 `coredump` partition decoded from
inside the merged flash image.

The `encrypted` flag gap was confirmed by reading both sides: `parse_partition_layout`
required `fields.len() >= 5` and constructed its entry from indices 0 to 4, and
`decode_partition_table` read bytes 2, 3, 4..8, 8..12, and 12..28 and never 28..32.
`espflash` does apply the flag, which the decoded table shows for `nvs_keys`; nothing
compared it.

Three probes made the new checks fail before they were trusted.

Removing `encrypted` from the `coredump` row produced "flash encryption is enabled and the
coredump partition is not marked encrypted, so ESP-IDF would write task stacks to flash in
plain text and only log a warning". The layout comparison did not fire, correctly: the
declaration and the image agreed, and the cross-check is what catches a declaration that is
wrong for the posture.

Declaring `captures_dram = true` and setting the matching `sdkconfig` key produced
"whole-DRAM core dump capture needs at least 131072 bytes reserved and the coredump
partition is 110592 bytes".

Changing the declared task watchdog period to 9 seconds while leaving the build at 5
produced "CONFIG_ESP_TASK_WDT_TIMEOUT_S must be 9, got 5".

Changing the mirrored `TASK_WATCHDOG_TIMEOUT_SECONDS` to 7 failed the pinning test with
`left: 5, right: 7`. All four probes were reverted and each reversion verified by search.

## Supersession

None. This extends the M0-008 firmware evidence bundle with an additive `boot_policy`
section and the M2-013 layout with one flag, and it refines the layout comparison M2-013
introduced.
