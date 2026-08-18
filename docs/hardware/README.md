# Hardware Manifests

One manifest per board this project targets. A manifest records what the hardware
*is*, with a citable source for every value, so later work can rely on it instead of
re-deriving it from marketing pages.

| Board | Manifest | Board revision | Schematic revision |
| --- | --- | --- | --- |
| Seeed Studio reTerminal D1001 | [reterminal-d1001.md](reterminal-d1001.md) | Main Board V1.0 | V01, 2025-10-15 |

## What belongs in a manifest

Values that are properties of the hardware and stable for a revision: part numbers,
connector designators and their functions, bus addresses, and the revision identifiers
themselves. Each entry names where it came from.

## What does not

Anything derived at build or run time. Flash layout lives in `firmware/partitions.csv`,
memory budgets in `ARCHITECTURE.md`, and toolchain pins in `toolchain/manifest.toml`.
A manifest that repeated those would drift from them.

## Separating sources from claims

A manifest distinguishes what a schematic shows from what a product page asserts. Where
the two disagree, or where a claim cannot be resolved to a component, the manifest says
so rather than choosing the more convenient reading. Those gaps are the manifest's most
useful content, because they are what would otherwise be discovered on a bench.
