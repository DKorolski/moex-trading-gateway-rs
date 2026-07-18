# Stage 5D-b2b-d1-r3 — source-produced runtime-restored closure hardening

Status: implementation candidate.

Stage 5D-b2b-d1-r3 closes the review findings from the first controlled
runtime-state-restored transition without opening any operational wiring.

## Scope

Allowed in this slice:

- Stage 5D runtime-state-restored transition hardening;
- checker-owned Stage 5C crate-private bridge additive region;
- focused b2bd/b2bd1 tests;
- Stage 5D additive checker and negative harness;
- documentation and review gate evidence.

Still closed:

- Redis;
- FINAM;
- broker transport;
- command dispatch;
- runtime-live;
- broker execution;
- real endpoint calls;
- actual order submission.

## Hardening implemented

- Callback intents are intercepted by the runtime guard before any
  `debug_assert!`, so debug/test/release all return the same controlled terminal
  outcome instead of panicking.
- The test hook injects a synthetic intent into the actual callback intent vector
  before the common production guard.
- Preflight now checks lifecycle chronology:

```text
admission.checked_ts
<= admission.issued_ts
<= bootstrap_notified_ts
<= restored_ts
<= admission.expires_at
```

- `persisted_at_ts_utc <= restored_ts` remains enforced.
- Broker side is exact before callback:

```text
broker qty > 0  -> current_side == Long
broker qty < 0  -> current_side == Short
broker qty == 0 -> current_side == None
```

- The Stage 5D bridge adds its own exact post-callback broker-truth validation
  while leaving the immutable Stage 5C baseline intact.
- Stage 5D negative harness inventory expands from `54` to `66` cases with
  marker-pinned checks for:
  - missing runtime intent guard;
  - guard moved after `debug_assert!`;
  - missing exact post-callback broker-truth guard;
  - missing bootstrap-notification timestamp guard;
  - missing exact flat-side guard;
  - missing source-produced current-shadow Long/Short/realized-PnL restored
    transition proof;
  - missing single-row and multi-row recovery restored transitions;
  - missing pre-callback state-fingerprint preservation;
  - missing compile-fail type-state guards.

## Focused evidence

Focused b2bd/b2bd1 unit tests cover:

- clean restored transition;
- exact admission expiry boundary;
- expired admission pre-callback block;
- incomplete recovery pre-callback block;
- controlled terminal callback intent path;
- exact bootstrap notification timestamp boundary;
- restored timestamp before bootstrap notification block;
- flat broker truth with stale `Long`/`Short` side blocked before callback.
- source-produced current-shadow Long/Short and realized-PnL states restore
  through the strict pipeline without post-injection mutation;
- single-tail and ordered multi-row recovery paths reach the restored
  Stage 5C capability exactly once;
- pre-callback blocked outcomes preserve the canonical strategy-state
  fingerprint while retaining the input capability;
- rustdoc compile-fail examples cover unconstructible capability, consumed
  input, non-substitutable restored output, outcome type-state separation and
  private bridge/preflight imports.

## Gate evidence

```text
review_gate_command: bash scripts/stage5d_b2bc_review_gate.sh
review_gate_capture: local stdout, not committed; reports/ remains excluded from handoff
review_gate_result: passed
```

The full gate ended with:

```text
stage5d-b2bd1-review-gate: all required gates passed
```

Pinned hashes:

```text
stage5d_manifest_sha256: daaf80a9710f5646cf4db259f83010503643d1af3996ca6876d33ce520234a6e
stage5d_checker_sha256: 54e8cdebfee04bc9227a5f44b3a235bf95e832f1e55cc36ad965328b1bf4956c
stage5d_negative_harness_sha256: 490ecb1537faec688af22ba208f213aa366d56637a9e8314869a2f4949989b49
forbidden_surface_scan_sha256: 8eef532650eab2b59328e46d2e79f7ff3c92bcf18ce40a311aff863cee66706a
stage5c_paper_host_sha256: b3a7b02815c7cd3ada1cb5976c88569d3128aa34226fb4271e73efb87fb58610
stage5d_persistence_sha256: e8793feb61e270c967ee849cc0b0726bbef7547fa89dff1b0e03ded062012ffa
review_gate_script_sha256: a7656932f6fed90a21d6c4bfae15f19b4a721fa0cfc7afd1e455017aa8632355
```

## Boundary statement

This slice only hardens the already opened no-I/O lifecycle transition. It does
not add Redis reads/writes, FINAM calls, transport, dispatch, runtime-live,
autonomous recovery workers or real order execution.
