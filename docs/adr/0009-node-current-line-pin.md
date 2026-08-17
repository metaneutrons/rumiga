# ADR-0009: Node Pin On The Current Release Line

- Status: Accepted
- Date: 2026-08-17
- Owners: @metaneutrons
- Task: M0-015

## Context

`TOOLCHAIN.md` justified the Node pin with "Web build LTS runtime", and `24.19.0`
was the newest release of the `Krypton` long-term-support line. Every other pin in
the manifest follows the same conservatism, because the evidence chain has to stay
reproducible over years.

Two facts made the choice worth revisiting. At the decision date the Node release
index lists 26 as `lts: false` with `26.7.0` released on 2026-08-05, and Node 26 is
expected to enter long-term support around October 2026. Staying on 24 therefore
means stepping the pin twice within a few months, once now for security releases
and again when 26 becomes LTS.

The type definitions were also wrong in a way the LTS discussion had hidden.
`@types/node` was pinned to `^22.20.1` while the runtime was `24.19.0`, so the
types described a Node major the build never used.

## Decision

Node is pinned to `26.7.0` and npm to `11.19.0`, the pairing the release index
records for that release. `@types/node` moves to `^26.2.0` so the definitions match
the runtime.

This is a deliberate, documented exception to the long-term-support rule rather
than an oversight, and `TOOLCHAIN.md` now says so at the point where the previous
rationale stood.

Two properties make the exception affordable.

Node is a build-time tool only. It produces the static export under `web/out`,
which is embedded into the desktop binary and later into the firmware. No Node
runtime ships in the product, so a defect in a newer Node cannot reach a user; it
can only break a build, which a gate catches.

Nothing in the dependency graph forbids it. `next@16.3.1` declares
`node >=20.9.0` and `eslint@9.39.5` declares `^18.18.0 || ^20.9.0 || >=21.1.0`.

## Consequences

Types and runtime agree for the first time. The pin will not need a second move
when 26 becomes LTS.

The cost is that a current line receives breaking changes and shorter support than
an LTS line, so pin refreshes may be needed more often until October 2026. That
cost falls entirely on the build, never on a shipped artifact.

The single-source pin structure absorbs the change cleanly:
`toolchain/manifest.toml` is authoritative, and
`firmware/tests/toolchain_manifest.rs` proves that `.node-version`,
`web/package.json` engines, and `packageManager` agree with it. A partial update
fails a test rather than producing a build that disagrees with its own manifest.

This decision does not weaken the rule for any other pin. Rust, ESP-IDF, the ESP
crates, and the firmware tools remain on their conservative selections, and their
rationale is unchanged.

## Alternatives

Staying on 24 until October 2026 was the conservative option and was rejected
because it buys no product safety, since Node never ships, while guaranteeing a
second pin move within months.

Moving only `@types/node` to `^24` was the minimal correction and would have fixed
the type mismatch alone. It was rejected because it leaves the double pin move in
place; it remains the right fallback if the current line proves disruptive.

Tracking the newest Node line continuously was rejected. The pin stays exact and
moves only by decision, because reproducible evidence depends on an exact version
rather than a range.

## Evidence

On Node `26.7.0` with npm `11.19.0`, `npm ci` installs 355 packages,
`npm run lint` is clean, and `npm run build` produces the five static routes.
`cargo +1.97.1 test --locked -p rumiga-firmware` passes the cross-file pin test.
The Node release index was read directly for the LTS status and the npm pairing
rather than taken from memory.

Clean pull-request run
[`32070931258`](https://github.com/metaneutrons/rumiga/actions/runs/32070931258)
produced governance artifact `9301688782` with archive SHA-256
`e9e60d858b18d0b946500cd52855630edacfff694e828b5df4fc598486bcd1b8`. Final `main`
run
[`32072021615`](https://github.com/metaneutrons/rumiga/actions/runs/32072021615)
produced governance artifact `9302065297` with archive SHA-256
`eeaf3756244b1fcd6f1bc45d2b530efe9f910cdf0a1004e65fa32a58205f80c3`. Both were
independently downloaded and verified, and both host legs installed Node `26.7.0`
on a foreign runner before building the web export.

## Supersession

None. This records an explicit exception to the pin conservatism that ADR-0001's
toolchain discipline established and narrows it to Node alone.
