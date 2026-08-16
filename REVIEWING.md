# Reviewing Rumiga Changes

Review protects emulator correctness, D1001 feasibility, user data, and the
truthfulness of compatibility claims. Review the change record and pull request
before reading individual lines so the intended contract is clear.

## Review Order

1. Confirm task, scope, non-goals, and change-record links.
2. Check behavior, failure handling, and ownership boundaries.
3. Check tests and evidence against the claims being made.
4. Check embedded resource bounds and portability.
5. Check security, private-media handling, and dependency policy.
6. Check documentation, release note, ADR, and rollback accuracy.

Report findings by severity:

- **P0**: active compromise, certain data loss, or unusable protected branch.
- **P1**: likely correctness, security, compatibility, or corruption defect.
- **P2**: meaningful reliability, maintainability, test, or contract gap.
- **P3**: non-blocking improvement with concrete value.

## Correctness

Trace inputs through state transitions and externally visible results. Check
reset behavior, timing boundaries, integer widths, endian assumptions, partial
I/O, cancellation, retries, and error propagation. New abstractions must remove
real complexity or match an established repository boundary.

## Compatibility And Evidence

Do not accept a feature claim from a screenshot alone. Verify the scenario,
configuration, ROM/media hashes where private evidence is used, reference
emulator/version, source revision, dirty flag, and checksums. Missing private
media is a skip, never a pass. Host evidence cannot promote D1001 behavior.

## Embedded Constraints

Reject unbounded media loads, queues, allocations, retries, logs, or task
creation in target paths. Check `no_std + alloc` compatibility where required,
32-bit arithmetic and alignment, PSRAM/internal-RAM placement, watchdog impact,
latency, and backpressure. Unsafe ESP-IDF boundaries require an accepted ADR,
documented invariants, and safe public APIs.

## Security And Supply Chain

Check path confinement, authentication assumptions, request limits, secret
redaction, media write policy, and recovery behavior. Dependencies and Actions
must satisfy `DEPENDENCY_POLICY.md`, immutable-source requirements, licenses,
and advisory gates. Do not approve private paths or copyrighted bytes in public
artifacts.

## Documentation And Traceability

The task, change record, tests, evidence, release note, ADR, and status documents
must agree. Reject stale counts, aspirational claims written as current fact,
and evidence from another revision. User-visible controls must remain aligned
across CLI, REST, web UI, persistence, and support bundles where applicable.

## Decision

Approve only when no blocking finding remains, required checks pass on the
final revision, and rollback is credible. Use comments for questions and P3
suggestions; request changes for P0-P2 findings. A reviewer may require a split
when independent behavior cannot be evaluated or reverted independently.
