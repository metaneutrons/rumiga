# Governance Records

`changes/` contains the machine-readable audit link for material engineering
tasks. Each JSON file conforms to `change-record.schema.json` and connects one
stable implementation-plan task to scope, tests, evidence, documentation,
release notes, architecture decisions, risk, and rollback.

Use status `planned` before implementation, `implemented` while hosted evidence
is pending, `verified` after the required evidence has been independently
checked, and `released` only through the future release process. A `verified`
or `released` record may not contain a pending evidence location.

The Rust validator is authoritative for CI because it also verifies repository
paths and cross-file relationships. The JSON Schema provides editor and tooling
support; changes to either contract must keep the validator tests green.
