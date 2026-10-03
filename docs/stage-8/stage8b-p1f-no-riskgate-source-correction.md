# BO-only no-riskgate / short-warmup source correction

Date: 2026-09-30. Status: SOURCE ACCEPTED at
`2f46491a4f63249f64676306d5f9c3445818f4f9`; no operational activation.
Predecessor: `3923c5c94f27a1dc980297f289c10ecca652f99b`.
Review: `FINAM_2f46491_NO_RISKGATE_SOURCE_REVIEW_2026-09-30.md`.
Review SHA-256: `eeeddf2155d47545e9fc107bd1326859e9a42e2bcaf0850120e864a0fcebf4d1`.

Direction: [owner ADR](../adr/adr-stage8b-bo-only-no-riskgate-short-warmup.md).
Evidence/model scope: [handoff reconciliation](stage8b-p1f-bo-only-handoff-alignment-2026-09-30.md).

## Implemented boundary

This is additive profile selection, not a relaxation of the old profile.
The table records accepted 2f46491. Its elapsed 14-day fetch cap has a calendar
boundary defect addressed by the separately authorized [P2 successor](stage8b-p1f-short-history-range-correction.md):
transport envelope <=15 days, while session age <=14 and four prior sessions
remain unchanged. That successor was separately SOURCE ACCEPTED at `8e7a647`;
it is not retrospectively part of the old acceptance.

| Boundary | Retained legacy | Explicit no-riskgate successor |
|---|---|---|
| Runtime profile | [V1](stage8b-p1e-runtime-profile-v1.json), existing hash and constructor | [V2](stage8b-p1e-runtime-profile-v2.json), own ID/hash/config fingerprint |
| Source bundle | Wire V2, 121 history / 120 riskgate minimum | Wire V3, four complete prior sessions and exact closed current-session prefix |
| Source plan | [V2](stage8b-p1e-first-boot-source-plan-v2.json), unchanged | [V3](stage8b-p1e-first-boot-source-plan-v3.json) |
| Riskgate | Existing High180 oracle and exact observations | Disabled mode, canonical empty observations, no High180 call/seed/ledger import |
| Materialization policy | V1, 180–400 calendar days | V2, positive range at most 14 calendar days; exact V2 profile pair required |
| Recovery | Existing identity/provenance retained | Exact new profile + source-plan + runtime fingerprint; cross-profile restore rejected |

Profile V2 canonical SHA-256:
`6d7dff3543993b7727a161f85af3037f74762ecd2ce8332e0594b9c92d987c38`.
Source plan V3 file SHA-256:
`d722d70a897578ce93217f34c82dff2a7ed6c6c402a12914b7b862c2d95b693c`.

Baseline07 model times, BO coefficients, MR-entry suppression, market sizing and
accepted paper execution policy remain unchanged. The model profile is explicitly
`baseline_runtime_hybrid`, with gate policy/mode disabled. Dormant High180/MR
parameter serialization is retained for compatibility; no High180 warmup is run.
Candidate09/weekend-state/current-close fills are not enabled.

## History and disabled-state representation

Four complete prior weekday sessions must lie within 14 calendar days of the
candidate. The current date has separately declared full session windows;
materialization trims only their exact candidate-exclusive prefix. Missing bars
do not create inferred clearing breaks. Source admission rechecks this prefix,
coverage hash, ordering and exact required close timestamps.

The bounded profile applies to baseline07 dates from 2026-07-14 onward. Complete
day endpoints are 07:10 and 23:50 MSK canonical closes. The producer's explicitly
reviewed calendar windows specify intervening breaks/holidays; this slice is
not a universal exchange-calendar verifier or an early-close-session policy.
Four sessions do not mean four calendar days. The 14-day ceiling is a finite
fetch budget, not an instruction to fetch all 14 days or expand history on failure.
If the explicit calendar cannot meet this policy, admission fails for review.

Wire V3 retains a compatibility-shaped `riskgate_history` object with
`source_mode=disabled-bo-only-v1`, `state_generation=disabled-v1`, exact history
hash, `session_observations=[]` and its canonical hash. It must not contain even
one fabricated zero-PnL row. Internal restart schemas retain their empty riskgate
section: no ledger rows, current shadow session, rolling sum, gate decisions,
pending finalizations or outbox. This is a verified disabled representation,
not successful High180 accounting. Nonempty rehashed state is rejected.

Fresh source admission checks candidate age against the trusted admission clock;
authenticated historical transaction recovery retains the original capture-time
check. Freshness is not bypassed merely by supplying an old capture timestamp.

## Review evidence

Run `python3 scripts/stage8b_p1f_no_riskgate_source_gate.py` locally. It retains
exact commands, exit codes, logs and a before/after source inventory under
`reports/stage8b-p1f-no-riskgate-source/`. It runs core tests, durable-service
library tests, gateway library/materializer CLI tests, formatting and strict
production lib/bin clippy with the existing `stage8b-r2a7-source-adapter`
composition feature (compile only, no publisher execution). The default gateway
feature set leaves three pre-existing publisher functions unused; this correction
does not suppress those warnings or edit the old authority modules. Optional
operational Redis endpoints are not inherited.
No full all-features or operational Redis/VPS qualification is claimed here.

Focused tests cover short bootstrap, intraday/clearing prefixes, missing bars,
forged disabled observations, old/new wire mismatch, capture/admission freshness,
pure export/restore and filesystem transaction/ordinary restart with exact
profile/plan binding. Legacy long-history and High180 tests remain in place.
Gate logs, not this document, are authoritative for executed counts/results.

At the accepted source commit the GOV-CI authority still described the predecessor.
Its actual exit 1 remains in the immutable source evidence and is not relabeled
PASS. The [governance successor](stage8b-p1f-no-riskgate-governance-closure.md)
rebinds the accepted source plus the separate `7c80d70` test-import build fix;
build-fix/closure review and fresh required CI
must precede merge/artifact deployment.
CI workflows, branch rules and old source manifests are not changed.

## Artifact and operational handoff

The new [template example](stage8b-p1f-o2-no-riskgate-source-template.example.json)
and [policy example](stage8b-p1f-o2-no-riskgate-policy.example.json) use fixture
identities and frozen September dates. They are **not deployment payloads**.
The later exact artifact must bind a newly validated operational identity,
profile fingerprint, selected calendar/cutoff, supervisor config and installation
manifest. Old installed template/policy files remain byte-exact.

Next: scoped authority closure review + fresh required CI → full exact artifact → separately
authorized terminal-history-preserving installation → separately authorized O2.
Retain Failed generation 1 / sequence 4 and preceding Expired/2 history; neither
is reset or replaced by a new genesis. O2 remains HOLD. P0/VPS unchanged in this
source task; FINAM order writes, broker dispatch, runtime-live and real orders
remain closed. No new recovery framework is introduced.
