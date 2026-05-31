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

## Current Classification Rules

- `edge_integrity.mirrored_non_background_pixels == 0` means the first visible
  lines did not show the reported right-edge pixels injected at x=0.
- Non-zero mirrored edge pixels are a display regression candidate and should be
  compared against FS-UAE from the same ROM/HDF inputs before changing host
  scaling behavior.
- A requester asking for `LIBS/workbench.library` is classified as a media or
  install-state result for the supplied HDF, not as a Gayle/IDE boot failure.
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
