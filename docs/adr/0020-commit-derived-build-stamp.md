# ADR-0020: Commit-Derived Build Stamp

- Status: Accepted
- Date: 2026-08-18
- Owners: @metaneutrons
- Task: M2-002

## Context

M2-002 asks for a reproducible firmware build, with CI producing an ELF, a binary, a map,
a size report, and checksums. That bundle already exists. M0-008 built it and every run
since has published it, so the acceptance criterion as written was met before this task
started. The open word was "reproducible", and it turned out to describe something other
than what a first reading suggested.

Comparing two hosted bundles whose firmware inputs were byte-identical showed differing
ELF, bootloader, merged image, and map. The differing strings looked like a build clock,
`20:22:00` against `20:33:16`, and the obvious reading was that the build embeds the wall
clock and is therefore not reproducible.

That reading was wrong. Those are the two revisions' commit timestamps. `xtask` already
exports `SOURCE_DATE_EPOCH` from the HEAD commit, GCC derives `__DATE__` and `__TIME__`
from it, and ESP-IDF fills both descriptors from those macros. The stamp is a function of
the revision.

Measured directly, two builds of one revision from an emptied build tree agree byte for
byte in the bootloader, the partition table, the ELF, the merged image, the size report,
the resolved `sdkconfig`, and the flasher arguments. Only the linker map differs.

So the property largely held. What did not exist was any check that it holds. Nothing in
the repository would have noticed if `SOURCE_DATE_EPOCH` stopped being set.

## Decision

Pin the inputs that decide reproducibility, in the evidence task, on every run.

The application descriptor's `date` and `time` must equal the HEAD commit time rendered
the way GCC renders it, and the bootloader descriptor's `date_time` must equal the two
joined. The descriptors are located by their documented magic values rather than by
searching for a date-shaped string, and the bootloader's single magic byte is paired with
its recorded ESP-IDF release so the match is specific.

The application descriptor's `version` must equal `git describe --always --tags --dirty`,
so the image names the source it was built from.

The merged image's `app_elf_sha256` must equal the digest of the packaged ELF, so a bundle
cannot pair one revision's image with another revision's ELF.

Record the linker map's one non-deterministic component rather than ignoring it. The map
embeds rustc's random temporary directory for the synthesized `symbols.o`, nineteen times
in the current build, and the manifest records that count.

Prove byte equality by rebuilding, on demand rather than on every run.
`cargo xtask firmware-evidence --verify-rebuild` empties the build tree, builds the same
revision again, packages a second bundle, and compares them artifact by artifact. The
manifest then carries which artifacts were identical and which needed the map
normalization. Without the flag the manifest claims `rebuild-not-compared`, so a bundle
cannot read as proving reproducibility when it only pinned the stamp.

Do not enable `CONFIG_APP_REPRODUCIBLE_BUILD`.

## Consequences

The regression that would break reproducibility now fails the build at the point it
happens, with a message that names the cause. Removing the `SOURCE_DATE_EPOCH` export
makes the task report the builder's local time against the commit time and stop.

The stamp is documented as provenance rather than noise. It also removes a timezone
dependency: without `SOURCE_DATE_EPOCH` the stamp is the builder's local time, which is
not merely non-deterministic but misleading, because it reads as a UTC field.

The full proof is not continuous. The firmware job would roughly double, from about six
and a half minutes to twelve, on every pull request, to re-derive a property whose only
known input is already checked. The rebuild comparison is therefore run out of band and
its result recorded, which is a weaker guarantee than a gate and is recorded as one.

A dirty tree still stamps HEAD's commit time while the sources differ from HEAD. The
`-dirty` suffix in the version field is what flags this, and CI rejects a dirty tree
outright.

The manifest's `source_dirty` and the descriptor's `-dirty` suffix count different things.
`git status --porcelain` counts untracked files, `git describe --dirty` does not, so a tree
with only untracked files reports `source_dirty` true and a version without the suffix.
Both are accurate about what they measure, and CI never sees the case.

The map normalization is a place where a future mistake could hide a real difference. It
is anchored on the full path shape, `/deps/rustc` plus six alphanumeric characters plus
`/symbols.o`, because a loose search for `rustc` and six characters also matches stable
text elsewhere in the same file.

The rebuild reuses the first build's directory rather than a second one. A differently
named directory would put different absolute paths into the map, and the comparison would
then need a normalization wide enough to hide a genuine environmental difference.

## Alternatives

Enabling `CONFIG_APP_REPRODUCIBLE_BUILD` was rejected. It removes date, time, and path
information from the image, and it was the plan until the measurement. It would delete a
deterministic provenance field to fix a problem that does not exist here, and the field it
removes is the one that lets a reader date an image to its commit.

Rebuilding on every CI run was rejected on cost. It is the strongest form of the claim,
and it doubles the longest job to re-derive a property whose one input the cheap check
already pins.

Rebuilding only on `main` was rejected. Branch protection is strict, so `main` and the
pull-request content are the same tree; a break would surface only after the merge, as a
broken `main`.

Comparing against digests pinned in the repository was rejected. Every legitimate change
to the firmware, the toolchain, or the lockfile would change them, so the pin would be
churn rather than a check.

Excluding the linker map from the comparison was rejected. Naming its one variable
component and normalizing exactly that says more than dropping the file, and it keeps a
real change to the map visible.

## Evidence

`riscv32-esp-elf-gcc` from `esp-15.2.0_20251204`, compiling a file containing
`__DATE__ " " __TIME__`, produced `Aug 18 2026 20:22:00` for
`SOURCE_DATE_EPOCH=1787084520` and `Aug 18 2026 20:42:09` for `1787085729`, which are the
commit timestamps of `bc47bb5` and `77fefa7`. With the variable unset it produced the
builder's local wall clock.

Two builds of one revision from an emptied build tree, on a clean tracked worktree,
produced identical bootloader, partition table, ELF, merged image, size report,
`sdkconfig`, and flasher arguments, and a map identical after the anchored normalization.
The stamp both runs recorded equalled that commit's timestamp to the second.

Removing the `SOURCE_DATE_EPOCH` export made the task fail with the application
descriptor reporting `Aug 18 2026 23:51:13` against a commit time of `Aug 18 2026
21:35:22`. The export was restored and the restoration verified by search.

Two independent implementations count nineteen random link paths in the map, the Rust
scan in the evidence task and a Python scan used to check it.

## Supersession

None. This refines the M0-008 firmware evidence bundle, whose `rumiga.firmware.build.v1`
manifest gains an additive `determinism` section. That follows the practice M0-014 set
when it added `merged_image` without a version bump.
