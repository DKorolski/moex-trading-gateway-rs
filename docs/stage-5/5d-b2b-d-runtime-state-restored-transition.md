# Stage 5D-b2b-d — controlled runtime-state-restored transition

Status: implementation candidate; hardened by Stage 5D-b2b-d1.

Stage 5D-b2b-d opens one narrow no-I/O transition:

```text
Stage5dRiskGateInjectedPaperStrategy
→ stage5d_notify_runtime_state_restored(...)
→ Stage5cRuntimeStateRestoredPaperStrategy
```

The transition is allowed only after private runtime state, broker truth,
authoritative riskgate evidence and the deterministic recovery plan are complete.
It does not open Redis, FINAM, broker transport, command dispatch, runtime-live,
autonomous recovery workers or real execution.

Stage 5D-b2b-d1 keeps this boundary and adds review-closure hardening for
callback-intent handling, bootstrap-notification timestamp ordering and exact
flat/long/short broker-side truth.

## Implemented boundary

- Public entry point:
  `stage5d_notify_runtime_state_restored(Stage5dRiskGateInjectedPaperStrategy)`.
- Successful output is the exact accepted Stage 5C
  `Stage5cRuntimeStateRestoredPaperStrategy`.
- Pre-callback failures return `Stage5dRuntimeStateRestoreOutcome::Blocked`.
  The opaque input capability is retained internally because no callback or
  mutation has occurred.
- Post-callback failures return `Stage5dRuntimeStateRestoreOutcome::Terminal`.
  No retry capability is exposed because the consumed capability has already
  crossed the source callback boundary.
- Runtime-state-restored callback intents are rejected in release mode. A
  non-empty intent vector is terminal and is never sent, published or dispatched.
- Production captures one `Utc::now()` timestamp and reuses it for preflight,
  callback context and restored receipt.
- Tests use crate-private deterministic `_at` helpers.

## Required preflight

Before the callback, Stage 5D validates:

- recovery plan is complete and every decision is already acknowledged;
- runtime pending riskgate finalizations are empty;
- recovery-plan fingerprint and envelope/evidence binding still match;
- restored known-order and pending-request indexes match the envelope;
- strategy/account/instrument/profile binding matches the Stage 5C admission;
- target broker-truth position agrees with semantic runtime state;
- protective broker-owned TP/SL/stop IDs are absent while that surface is closed;
- admission is still valid at the exact callback timestamp;
- the callback timestamp is not before the persistence lifecycle watermark;
- no Redis, FINAM, transport, dispatch, execution or runtime-live surface is
  enabled.

Preflight failure is recoverable only by restarting the controlled pipeline with
fresh valid inputs. The blocked diagnostic is intentionally redacted.

## Checker-pinned bridge

Stage 5D does not call the public Stage 5C restored transition directly. It uses
one crate-private bridge inside the Stage 5C additive region:

```text
stage5d_notify_runtime_state_restored_bridge_at
```

The Stage 5D checker and negative harness pin:

- exactly one bridge definition;
- exactly one production call site from `stage5d_notify_runtime_state_restored_at`;
- no aliases, function references, forwarding wrappers or extra Stage 5D calls;
- no public construction path for bridge inputs.

## Review gates

The b2b-d gate must include:

```bash
python3 scripts/stage5c_api_freeze_check.py
python3 scripts/stage5d_additive_freeze_check.py
bash scripts/forbidden_surface_scan.sh
python3 scripts/stage5d_additive_freeze_negative_harness.py
cargo test -p strategy-runtime-core b2bd --lib
bash scripts/stage5d_b2bc_review_gate.sh
```

The Stage 5D additive negative harness is expected to include the restored-bridge
boundary cases in addition to the c1-r8 cases. In b2b-d1 it also includes
marker-pinned semantic guard cases for intent guard ordering, timestamp
chronology and exact side truth.
