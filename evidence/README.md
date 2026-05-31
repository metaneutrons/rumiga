# Rumiga Evidence Scenarios

This directory documents reproducible compatibility evidence. It intentionally
does not contain Kickstart ROMs, Workbench media, HDFs, ADFs, generated
screenshots, or local reference captures.

Generated evidence belongs under `target/evidence/<scenario>/` by default. That
keeps local ROM/media paths and generated images out of git while making each
scenario easy to regenerate.

## A1200 Workbench HDF

Run the stock A1200 HDF evidence scenario with:

```sh
scripts/capture-a1200-hdf.sh
```

The script defaults to:

- ROM: `assets/kick.a1200.47.102.rom`
- HDF: `assets/workbench-39.hdf`
- CPU: `68020`
- frames: `4000`
- output directory: `target/evidence/a1200-hdf`

Override local paths without editing the repo:

```sh
RUMIGA_A1200_ROM=/path/to/kick.a1200.47.102.rom \
RUMIGA_A1200_HDF=/path/to/workbench.hdf \
RUMIGA_CAPTURE_FRAMES=4000 \
RUMIGA_EVIDENCE_DIR=target/evidence/a1200-hdf \
scripts/capture-a1200-hdf.sh
```

The scenario writes:

- `rumiga.png`: native framebuffer evidence after Rumiga viewport processing.
- `rumiga.json`: stable capture manifest with schema, producer, ROM/HDF hashes,
  model, CPU, viewport, display-window, and edge-integrity diagnostics.
- `notes.md`: generated local classification notes for the capture.

## A2065 SLIRP Network

Run the A2065 + SLIRP evidence scenario with:

```sh
scripts/capture-a2065-slirp.sh
```

The script defaults to:

- ROM: `assets/kick.a1200.47.102.rom`
- HDF: `assets/workbench-39.hdf`
- CPU: `68020`
- MAC: `00:80:10:4d:49:47`
- frames: `4000`
- output directory: `target/evidence/a2065-slirp`
- PCAP: `target/evidence/a2065-slirp/rumiga.pcap`

Override local paths without editing the repo:

```sh
RUMIGA_NETWORK_ROM=/path/to/kick.a1200.47.102.rom \
RUMIGA_NETWORK_HDF=/path/to/workbench-with-network-stack.hdf \
RUMIGA_NETWORK_MAC=00:80:10:4d:49:47 \
RUMIGA_NETWORK_PCAP=target/evidence/a2065-slirp/rumiga.pcap \
RUMIGA_CAPTURE_FRAMES=8000 \
RUMIGA_EVIDENCE_DIR=target/evidence/a2065-slirp \
scripts/capture-a2065-slirp.sh
```

The default `link` mode proves the emulator-side contract: A2065 is present,
SLIRP is enabled, link state is up, and the manifest records packet counters.
It does not require the supplied HDF to contain a TCP/IP stack. The scenario
also writes a raw Ethernet PCAP for guest TX and SLIRP RX frames when packets
exist.

For guest-side TCP proof, provide an HDF that boots an A2065/SANA-II driver and
network stack, then run strict mode:

```sh
RUMIGA_NETWORK_EVIDENCE_MODE=guest-tcp \
RUMIGA_NETWORK_EXPECT_A2065_CONFIGURED=1 \
RUMIGA_NETWORK_EXPECT_TX_MIN=1 \
RUMIGA_NETWORK_EXPECT_RX_MIN=1 \
scripts/capture-a2065-slirp.sh
```

Static SLIRP settings matching the WinUAE/FS-UAE user-mode NAT default are:

- IP: `10.0.2.15`
- Gateway: `10.0.2.2`
- DNS: `10.0.2.3`
- Netmask: `255.255.255.0`

The guest proof workload should include gateway ping, DNS lookup, HTTP fetch,
and checksum validation. Packet captures must be local-fixture only or redacted
before sharing.

## Compatibility Report

Generate a local release-style report from all available evidence manifests:

```sh
scripts/generate-compatibility-report.py
```

The script defaults to:

- evidence root: `target/evidence`
- output: `target/evidence/compatibility-report.md`

Override paths for release-candidate bundles:

```sh
scripts/generate-compatibility-report.py \
  --evidence-root target/evidence \
  --output target/evidence/compatibility-report.md
```

Use `--strict` in automation when a failed scenario should fail the job. A
`partial` result is allowed for cases that are useful evidence but not full
feature proof yet, such as A2065 link/configuration without guest TCP packets.

## Current Classification Rules

- `edge_integrity.mirrored_non_background_pixels == 0` means the first visible
  lines did not show the reported right-edge pixels injected at x=0.
- Non-zero mirrored edge pixels are a display regression candidate and should be
  compared against FS-UAE from the same ROM/HDF inputs before changing host
  scaling behavior.
- A requester asking for `LIBS/workbench.library` is classified as a media or
  install-state result for the supplied HDF, not as a Gayle/IDE boot failure.
- `a2065-link-ready-awaiting-guest-driver` means SLIRP and A2065 are enabled,
  but the current HDF did not autoconfigure/use the card within the frame
  budget.
- `guest-tcp-evidence` requires a guest network stack to configure A2065 and
  meet the packet-counter thresholds set by the scenario.
- Compatibility reports classify a scenario as `fail` for schema drift,
  mirrored viewport edges, active boot workarounds, unusable RDB geometry, or an
  incomplete required A2065/SLIRP contract.
- Compatibility reports classify a scenario as `partial` when the emulator path
  is usable but not fully proven, for example A2065 configured with link up but
  zero guest packet counters.
- Host-window screenshots are useful for presentation bugs, but native
  framebuffer captures are the release gate for chipset viewport correctness.

## Evidence Notes

When adding a new scenario, keep it self-contained:

- Provide a command or script that works with relative defaults and environment
  variable overrides.
- Record the machine model, CPU, media inputs, frame count, and expected
  milestone.
- Keep reference captures under `target/evidence` or another ignored local path.
- Never commit commercial ROMs, Workbench media, or screenshots containing
  locally licensed assets unless the asset owner explicitly approves it.
