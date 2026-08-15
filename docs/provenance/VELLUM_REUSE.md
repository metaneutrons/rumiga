# Vellum Source Reuse Authorization

- Status: active
- Recorded: 2026-08-15
- Source repository: `https://github.com/metaneutrons/Vellum.git`
- Evidence baseline: `15bff64d316c3751861d02fcf7ace6b47afab176`
- Rumiga distribution license: `GPL-3.0-only`

## Authorization

The copyright holder of the original Vellum implementation has explicitly
authorized Rumiga to copy, adapt, reimplement, and redistribute the Vellum code
they own as part of Rumiga under `GPL-3.0-only`. Vellum's published
`AGPL-3.0-or-later` license remains recorded as source provenance, but it is not
a compatibility blocker for material covered by this copyright-holder
authorization.

This authorization does not change Vellum's published license. It does not
cover third-party code, generated vendor code, binary components, fonts,
images, firmware, or other assets for which the author does not hold the
necessary rights. Those inputs retain their original terms and require a
separate compatibility review before entering Rumiga.

## Reuse Controls

Every Rumiga change that copies or materially adapts Vellum implementation
code must:

1. identify the immutable Vellum revision and source path;
2. identify the Rumiga destination path;
3. distinguish owner-authored material from third-party or generated inputs;
4. preserve copyright and attribution notices where required;
5. pass Rumiga's formatting, lint, test, target-build, and affected HIL gates;
6. add an entry to the transfer register below in the same functional commit.

Behavior learned from Vellum without implementation reuse does not require a
transfer entry, but evidence documents must still cite the revision used.

## Transfer Register

No Vellum source transfer has been recorded in Rumiga yet.

When the first transfer is made, replace the sentence above with a table using
these columns:

| Date | Vellum revision and path | Rumiga path | Rumiga commit | Rights review |
| --- | --- | --- | --- | --- |
