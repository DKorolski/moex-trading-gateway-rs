# Stage 5D-b2b-a/b/c/c1-r5 — runtime-private apply, bootstrap and riskgate bridge

Status: Stage 5D-b2b-c1-r5 review-closure candidate.

Stage 5D-b2a closed the strict persistence schema and validated-envelope
capability. Stage 5D-b2b-a opened the first controlled implementation slice:
runtime-private extension export/apply. Stage 5D-b2b-b adds the next controlled
type-state transition: broker-truth bootstrap notification after private apply.
Stage 5D-b2b-c adds authoritative riskgate ledger evidence validation and
riskgate projection injection after broker-truth bootstrap and before the
runtime-state-restored callback. Stage 5D-b2b-c1-r5 hardens that same boundary;
it does not add the final restored transition.
Redis, FINAM, broker transport, command dispatch, runtime-live and real order
execution remain closed.

## Scope

Implemented:

- public `stage5d_bind_runtime_state_loaded(...)` transition;
- public `stage5d_apply_runtime_private_extension(...)` transition;
- public `stage5d_notify_broker_truth_bootstrap(...)` transition;
- public `stage5d_retry_broker_truth_bootstrap(...)` transition;
- bind input requires `Stage5dValidatedPersistenceEnvelope` plus a Stage 5C
  loaded runtime capability;
- apply input requires opaque `Stage5dEnvelopeBoundRuntimeStateLoaded`;
- output is opaque `Stage5dPrivateStateAppliedPaperStrategy`;
- recoverable block is represented by opaque
  `Stage5dRuntimePrivateApplyBlocked`;
- block exposes only redacted reason and preserves the input loaded capability
  internally;
- public `stage5d_retry_bind_runtime_state_loaded(...)` retries a recoverable
  block without exposing raw strategy state;
- successful apply retains the validated restore evidence privately and exposes
  only redacted `snapshot_id`, `schema_version` and `evidence_fingerprint`;
- bootstrap input requires the opaque `Stage5dPrivateStateAppliedPaperStrategy`
  produced by the apply transition;
- bootstrap output is opaque `Stage5dBootstrappedPaperStrategy`;
- bootstrap block is represented by opaque `Stage5dBootstrapBlocked`, exposes
  only redacted reason/snapshot id, and preserves the input applied capability
  internally;
- bootstrap retry consumes only `Stage5dBootstrapBlocked` plus a fresh
  `Stage5cPaperHostAdmission`; it replaces only authoritative broker-truth
  admission and does not expose or re-run runtime-private apply;
- public `stage5d_inject_authoritative_riskgate(...)` transition;
- public `stage5d_validate_riskgate_ledger_evidence(...)` transition;
- public `stage5d_retry_authoritative_riskgate_injection(...)` transition;
- riskgate injection requires opaque `Stage5dValidatedRiskGateLedgerEvidence`;
- riskgate injection input requires the opaque
  `Stage5dBootstrappedPaperStrategy` produced by the controlled bootstrap
  transition;
- riskgate injection output is opaque
  `Stage5dRiskGateInjectedPaperStrategy`;
- riskgate injection block is represented by opaque
  `Stage5dRiskGateInjectionBlocked`, exposes only redacted reason/snapshot id,
  and preserves the input bootstrapped capability internally;
- authoritative ledger evidence contains normalized source-compatible ledger
  records, full `RiskGateProfileIdentity`, ledger tail hash, seed/current-shadow
  metadata and current generation;
- generation must equal the source `RISK_GATE_STATE_GENERATION` and the exact
  envelope materialized generation; the record-tail hash retains its v1
  meaning while a separate redacted evidence fingerprint binds tail,
  seed/current-shadow metadata and generation;
- every ledger row is source-producible: date/session/source/status chronology
  is validated, source functions recompute all rolling and gate fields, and
  finalization timestamps must be valid, monotonic, at/after their session and
  not later than the envelope persistence timestamp;
- authoritative projection is rebuilt from all source-exact ledger evidence;
  exact durable outbox states first derive separate materialized and semantic
  prefix frontiers, then each local projection is compared to its permitted
  frontier before callback;
- all riskgate identity fields are checked against runtime config:
  strategy/profile/mr-variant/timeframe/session-policy/model-version;
- disabled/non-applicable riskgate runtime profiles fail closed instead of
  returning `riskgate_injected=true` after a source callback no-op;
- runtime pending riskgate finalizations are checked against a durable outbox
  state machine: generation, canonical identity hash, state, ledger-row
  presence and payload binding;
- the no-I/O outbox validator emits ordered deterministic recovery decisions:
  `AppendMissingLedgerRow`, `AdvanceToMaterialized`, `ReackRuntime` or
  `AlreadyAcknowledged`; prepared rows never plan a duplicate append and an
  acknowledged state always requires matching durable ledger truth;
- the opaque injection result retains a cryptographically bound private
  recovery plan; only redacted count/completion/fingerprint diagnostics are
  public, and no restored transition exists in this slice;
- decimal evidence must exactly round-trip through the source formatter and
  `seed_loaded` is derived from validated row provenance;
- actual riskgate callback is delegated through one checker-pinned crate-private
  Stage 5C bridge;
- wrapper additive region now exports/applies runtime-private DTO fields:
  pending entry payload, partial-entry timer, pending-exit context,
  bracket-reconciliation timer, cleanup retry state, last processed bar
  watermark and full runtime pending riskgate finalization vector.

Not implemented in this slice:

- final return to `Stage5cRuntimeStateRestoredPaperStrategy`;
- Redis-backed live ledger reads; Stage 5D-b2b-c uses deterministic in-process
  ledger evidence only;
- broker working-set authority restoration beyond fail-closed hint checking;
- active-order ownership mapping;
- stop-order broker-truth surface;
- Redis/FINAM/transport/dispatch/runtime-live.

## Working-set ownership

`expected_working_sets` remains a non-authoritative hint. The apply bridge does
not rehydrate runtime working maps from persistence. Actual active objects must
come from broker truth in a later gate.

Stage 5D-b2b-b checks these hints against the authoritative Stage 5C admission
broker snapshot before bootstrap notification:

- persisted position quantity must match target broker truth position quantity;
- expected working order ids must be present in target active broker orders;
- confirmed target active orders still fail closed at Stage 5C with
  `ActiveOrdersRequireOwnershipMapping` until ownership mapping is explicitly
  opened;
- expected working stop ids fail closed with
  `ExpectedWorkingStopUnsupported` until a broker stop-truth surface is opened.

If bootstrap blocks because the admission/broker snapshot is stale or
incomplete, Stage 5D-b2b-b now supports a controlled refresh path:

```text
Stage5dBootstrapBlocked + fresh Stage5cPaperHostAdmission
    -> stage5d_retry_broker_truth_bootstrap(...)
    -> Stage5dBootstrappedPaperStrategy | Stage5dBootstrapBlocked
```

The fresh admission must match the retained strategy/account/instrument/tick
binding and must not be older than the previous admission. Cross-account or
cross-instrument refresh attempts fail closed and preserve the original applied
capability.

## Pair-binding contract

Before runtime-private mutation, Stage 5D binds the validated envelope to the
specific loaded Stage 5C capability:

- admission strategy/account/instrument must match envelope binding;
- runtime Stage 5C config fingerprint and profile binding must match envelope
  binding;
- runtime Stage 5D canonical config fingerprint must match the authoritative
  Stage 5D binding;
- broker protocol/runtime state schema versions must match the supported
  Stage 5D-b2b-a table;
- Stage 5C loaded capability must carry crate-private `Persisted` load
  provenance, not `CleanStart`;
- persisted load provenance semantic/recovery fingerprints and persisted
  timestamp must match the envelope;
- source commit/build id must be accepted by the explicit Stage 5D semantic
  compatibility allowlist;
- current persisted-owned semantic `StrategyState` projection must match the
  envelope semantic payload projection; recomputable warmup/readiness/cache
  fields are retained for later gates but do not block private apply binding;
- loaded known order ids and pending requests must match the envelope recovery
  indexes.

The apply transition consumes only the bound capability. A clean prepared
capability cannot be used as a stand-in for a persisted restore, including flat
snapshots with empty recovery indexes.

The Stage 5D canonical config fingerprint now includes a runtime semantic
compatibility id plus hashed riskgate seed and ledger identities. Sensitive
identity strings are not stored in the fingerprint descriptor, but different
seed/ledger identities produce different canonical fingerprints.

Pending-entry private state is checked against source-created shapes before
mutation:

- `target_qty` must equal `config.qty.max(1.0)`;
- mean-reversion entries must be bracket entries, side-matched
  `MorningMeanReversionLong/Short`, with stop and take prices present;
- intraday-breakout entries must be market entries, side-matched
  `BreakoutLong/Short`, with no stop/take payload;
- partial-entry lifecycle is allowed only for mean-reversion bracket entries
  with config qty above one, valid sign/progress and a partial timer.

## Evidence

Regression tests prove:

- Stage 5C scalar restore initially exposes only the first pending riskgate
  finalization;
- Stage 5D runtime-private apply restores the full pending finalization vector;
- pending-entry private payload and partial-entry timer survive apply/export;
- invalid private-extension shape blocks before mutation and preserves the
  previous runtime export.
- account, instrument, semantic-state and recovery-index mismatches are blocked
  before private mutation;
- recoverable block can be retried with a corrected matching envelope without
  exposing the preserved capability;
- missing `cleanup_retry_state` is rejected for schema v1 and nonzero cleanup
  retry attempts roundtrip exactly.
- real Stage 5C restore with `entry_ready=true` persisted and `entry_ready=false`
  before warmup still binds;
- active-cycle/pending-request mismatches are blocked while recomputable field
  mismatches are retained for later warmup verification;
- Stage 5D canonical fingerprint and unsupported schema-version mismatches are
  blocked;
- unsupported source build/semantic compatibility ids are blocked while the
  accepted prior Stage 5D fixture build is explicitly allowlisted;
- clean-start flat capability cannot bind to a flat persisted envelope; a real
  persisted flat envelope with empty indexes does bind;
- persisted semantic/recovery provenance fingerprint mismatches are blocked;
- riskgate ledger/seed identity changes alter canonical config fingerprints;
- source-impossible private states are rejected before mutation: cleanup retry
  above source max, partial-entry sign/style violations, pending-entry
  target/config mismatch, MR/BO shape mismatches, pending-exit while flat/without
  active cycle, and bracket reconcile marker while flat.
- controlled broker-truth bootstrap succeeds after private apply on an exact
  flat persisted/broker snapshot;
- position drift between persisted semantic state and broker truth blocks before
  callback and preserves the applied capability;
- missing expected working orders block before callback;
- confirmed active working orders are matched, then still fail closed at the
  Stage 5C ownership-mapping boundary;
- expected stop hints fail closed until the stop-truth surface opens;
- expired admission preserves the applied capability and exposes only a redacted
  `AdmissionExpired` reason.
- expired admission can retry successfully with a fresh matching admission;
- missing expected order can retry with a fresh broker snapshot and then reaches
  the exact active-order ownership-mapping boundary;
- cross-account fresh admission is rejected and the blocked capability remains
  preserved.
- authoritative riskgate injection succeeds only after broker-truth bootstrap;
- semantic/materialized riskgate drift blocks before callback and preserves the
  bootstrapped capability;
- runtime pending riskgate finalizations missing from durable outbox block
  before callback and preserve the bootstrapped capability.
- full riskgate identity mismatches block: mr variant, timeframe, session
  policy and model version;
- disabled-profile callback no-op is rejected with explicit not-applicable
  reason;
- ledger tail hash drift is rejected before injection;
- materialized state is rebuilt from source-compatible ledger records;
- outbox crash-consistency rejects acknowledged runtime-pending records,
  acknowledged records without ledger truth, prepared records with mismatched
  existing rows, ledger/materialized records without matching rows, duplicate
  sessions/identities, reordered/zero generations, impossible state order and
  identity-hash mismatches;
- unknown/empty generation, evidence/envelope generation drift, every mutated
  derived row field, incomplete/source-order drift, invalid/decreasing/future
  finalization timestamps, and noncanonical numeric evidence fail closed;
- source-exact seed-prefix and runtime rows are accepted, and metadata changes
  alter the separate evidence fingerprint without changing the v1 tail hash;
- blocked riskgate injection can retry with fresh validated ledger evidence
  without repeating private apply or broker bootstrap.

The Stage 5D checker also pins the crate-private bootstrap and riskgate bridge
call-site contracts:

- `stage5d_bootstrap_preserving_loaded_at` may be defined exactly once in the
  Stage 5C additive region;
- production use is exactly one call inside
  `stage5d_notify_broker_truth_bootstrap_at`;
- `stage5d_inject_authoritative_riskgate_state` may be defined exactly once in
  the Stage 5C additive region;
- production use is exactly one call inside
  `stage5d_inject_authoritative_riskgate_with_evidence`;
- direct calls, aliases, forwarding wrappers, function references and extra
  Stage 5D calls are rejected by the negative harness.

The Stage 5D additive manifest now labels this baseline as `5D-b2b-c1-r5` and pins
the updated public API surface including the controlled bind/apply/bootstrap/
retry/riskgate-injection Stage 5D transitions. The formal surface policy records
`runtime_private_mutation =
controlled_validated_stage5d_apply_then_broker_truth_bootstrap_then_riskgate_injection_only`;
Redis, FINAM, transport, dispatch, runtime-live and broker execution remain
closed.
The manifest also records a controlled private-layout Stage 5C extension for
the crate-private persisted-vs-clean load provenance marker; Stage 5C public API
shape remains pinned by the Stage 5C compatibility checker. The private-layout
extension is checker-owned: exact path, `reason_id`, public-API flag and stripped
hash are pinned in `stage5d_additive_freeze_check.py`, and the negative harness
rejects removed/changed/extra extensions plus a self-authorized frozen semantic
drift attempt.

## Stage 5D-b2b-c1-r5 review gates

The r5 review gate summary is recorded in
`docs/stage-5/5d-b2b-c1-r5-review-gate-summary.md`.

`scripts/stage5d_b2bc_review_gate.sh` runs the Stage 5C and Stage 5D positive
checkers, normal and marker-pinned 81-case forbidden-surface gates, the isolated
bounded-parallel 44-case Stage 5D negative harness, no-Redis smoke, fixture parsing,
handoff source safety, copied-baseline completeness, and workspace
fmt/test/clippy. The manifest mutation policy remains exactly
`controlled_validated_stage5d_apply_then_broker_truth_bootstrap_then_riskgate_injection_only`.
The c1-r5 closure keeps the same no-I/O boundary, retains the c1-r4
negative-zero/current-shadow/recovery-frontier protections, and adds
source-canonical actual runtime pending-finalization export, source-bound
current-shadow chronology checks and generated handoff provenance verification.
The forbidden negative harness supported worker contract is pinned at
default/max four workers with a 30-minute CI timeout.

The accepted Stage 5C closure report and hashes remain immutable historical
evidence. Current candidate state is recorded here and in `current-status.md`;
the historical report is not rewritten.
