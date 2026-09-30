# BO-only no-riskgate — build correction and authority closure

Date: 2026-09-30. Status: SOURCE ACCEPTED / BUILD FIX + AUTHORITY REVIEW CANDIDATE.
Fresh required GitHub checks and history-preserving merge are separate pending gates.
No operational permission is introduced.

The accepted source failed canonical all-features Clippy. A separately authorized,
minimal conditional-import fix is recorded before the authority-only successor.
Local results are retained in the review package; they do not claim fresh GitHub
CI acceptance or independent acceptance of this correction/closure.

## Immutable acceptance

- Source: `2f46491a4f63249f64676306d5f9c3445818f4f9`.
- Source parent: `3923c5c94f27a1dc980297f289c10ecca652f99b`.
- Source tree: `19ab4c67281686173333f88d0116783a16b514f6`.
- Source ZIP: `moex-trading-project-2f46491-no-riskgate-source-review.zip`.
- ZIP SHA-256: `64d893b655369ff059c5e3beeaea565b2f4219413eb564ed6b21fcd72f2031ab`.
- Review: `FINAM_2f46491_NO_RISKGATE_SOURCE_REVIEW_2026-09-30.md`.
- Review SHA-256: `eeeddf2155d47545e9fc107bd1326859e9a42e2bcaf0850120e864a0fcebf4d1`.
- Verdict: SOURCE ACCEPT; no confirmed blocking P1/P2 in the reviewed delta.
- Build-only successor: `7c80d709679ccbf41d7f754364034df4bfafeb78`.
  Its sole changed file is `stage8b_p1e_first_boot_source.rs` (conditional import).

## Closure scope and evidence

Only `gov-ci-1-authority.json` inventories and status documents change relative
to the build-only successor. Relative to accepted `2f46491`, the package also
contains the explicitly separated conditional-import fix, not an authority-only
claim for that full delta. Rust/Cargo, machine-consumed profile/source-plan/policy
examples, workflow YAML, scanners/checkers, source gate, deployment files and
trust material remain byte-for-byte unchanged. The control-plane refresh is
limited to current-status and roadmap hashes; old accepted replay references,
compatibility pins, closed flags, required checks and negative inventory stay
unchanged relative to `7c80d70`. Runtime semantics are unchanged relative to the
accepted source. This is not a new strategy implementation or general CI redesign.

The handoff must reconstruct the closure, build-fix and accepted-source Git trees,
prove the allowed-only delta, retain the independent review bytes, and bind
fresh current-tree checker and negative-harness logs to the exact closure.
The original 2148 source tests (16 top-level ignored helpers) and 38/38 frozen
model rounds belong to the accepted source package. Fresh local checks and
their actual executed counts are recorded separately in the new handoff gate;
old results are not relabeled as new runs. Model current-close simulation is not
FINAM fill/latency/commission parity. The old source authority failure remains
in that immutable source evidence; the closure has its own new result.

Local closure verification does not replace canonical GitHub `rust` and
`redis-smoke`. The workflow runs on PRs and main pushes: publish the existing
source/closure branch for ordinary PR checks without modifying main, suppressing
gates, bypassing rules or enabling auto-merge. Merge waits for required green
checks and closure review. The PR also includes the already accepted `3923c5c`
artifact documentation/tools because that predecessor is not yet on main;
closure-only scope is measured against `7c80d70`, not against main; the complete
review delta from `2f46491` also includes that one-file build correction.

## Next bounded development

### Local CI blocker and separately authorized correction

`cargo clippy --workspace --all-targets --all-features -- -D warnings` fails
with an unused `Stage8bP1RuntimeProfileV1` import at
`crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs:37`.
The import is enabled by `stage8b-p1-test-fixtures`, but the type is used only
inside the unit-test module. The profile hash constant sharing that import is
also needed by feature-enabled fixture code. The accepted source gate used a
narrower feature selection, so its retained PASS does not prove this canonical
CI command passes.

The correction restricts the type import to `cfg(test)` while
retaining the constant's existing test-or-fixture-feature condition. This is a
separate source/build correction, not an authority-only change. The project owner
explicitly approved this narrow expansion on 2026-09-30. The complete canonical
Clippy command now passes locally. The new gate additionally exercises both
default and fixture-enabled durable-service tests and retains exact-tree logs.
No lint suppression, CI feature reduction or canonical workflow change is used.
Fresh GitHub checks, review and merge remain pending.

### Artifact and operational gates

After closure acceptance and synchronization, rebuild the full O2 artifact
from the exact synchronized baseline: supervisor, materializer/operator,
profile/source identities, artifact validation, current calendar/policy and
installation package must all agree on
`imoexf-baseline07-bo-only-no-riskgate-paper-v2`.
Profile SHA-256: `6d7dff3543993b7727a161f85af3037f74762ecd2ce8332e0594b9c92d987c38`.
Source-plan V3 SHA-256: `d722d70a897578ce93217f34c82dff2a7ed6c6c402a12914b7b862c2d95b693c`.

Select and review the latest four actual complete sessions and the full current
day windows at artifact preparation. The dated fixture examples are not deployable.
The 14-day fetch cap is not a mandate to fetch all 14 days. Missing required
recent M1 still fails closed; do not restore the obsolete 121-session requirement
or infer a clearing break from missing data. No universal calendar service or
new recovery framework is requested by this closure.

Artifact review precedes separately authorized installation, which precedes a
separately authorized bounded O2. Read and reconcile actual terminal history
before either operational step: preserve accepted `EXPIRED / 1 / 2` and all
later events, including retained `FAILED / 1 / 4`. The latter attempt's raw
evidence was not accepted by this source review; it remains a separate evidence
boundary. Never reset history or create a replacement genesis.

Then proceed to limited paper windows with ALOR comparison, and only later to
separate live-micro acceptance. VPS/P0 and installed bytes are unchanged here.
FINAM order POST/DELETE, broker dispatch, runtime-live and real orders remain closed.
