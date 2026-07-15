# Stage 5D-b2b-a — runtime-private export/apply bridge foundation

Status: review candidate.

Stage 5D-b2a closed the strict persistence schema and validated-envelope
capability. Stage 5D-b2b-a opens the first controlled implementation slice:
runtime-private extension export/apply, still without Redis, FINAM, broker
transport, command dispatch, runtime-live or real order execution.

## Scope

Implemented:

- public `stage5d_bind_runtime_state_loaded(...)` transition;
- public `stage5d_apply_runtime_private_extension(...)` transition;
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
- wrapper additive region now exports/applies runtime-private DTO fields:
  pending entry payload, partial-entry timer, pending-exit context,
  bracket-reconciliation timer, cleanup retry state, last processed bar
  watermark and full runtime pending riskgate finalization vector.

Not implemented in this slice:

- Stage 5D bootstrap wrapper;
- authoritative riskgate injection;
- final return to `Stage5cRuntimeStateRestoredPaperStrategy`;
- broker working-set authority restoration;
- Redis/FINAM/transport/dispatch/runtime-live.

## Working-set ownership

`expected_working_sets` remains a non-authoritative hint. The apply bridge does
not rehydrate runtime working maps from persistence. Actual active objects must
come from broker truth in a later gate.

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

The Stage 5D additive manifest now labels this baseline as `5D-b2b-a` and pins
the updated public API surface including the controlled bind/apply/retry Stage
5D transitions. The formal surface policy records
`runtime_private_mutation = controlled_validated_stage5d_apply_only`; Redis,
FINAM, transport, dispatch, runtime-live and broker execution remain closed.
The manifest also records a controlled private-layout Stage 5C extension for
the crate-private persisted-vs-clean load provenance marker; Stage 5C public API
shape remains pinned by the Stage 5C compatibility checker. The private-layout
extension is checker-owned: exact path, `reason_id`, public-API flag and stripped
hash are pinned in `stage5d_additive_freeze_check.py`, and the negative harness
rejects removed/changed/extra extensions plus a self-authorized frozen semantic
drift attempt.
