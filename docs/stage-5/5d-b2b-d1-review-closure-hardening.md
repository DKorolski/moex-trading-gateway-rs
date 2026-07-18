# Stage 5D-b2b-d1-r4 — broker-position and recovery-index closure hardening

Status: implementation candidate.

Stage 5D-b2b-d1-r4 closes the review findings from the first controlled
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
- Stage 5D negative harness inventory expands from `66` to `78` cases with
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
  - missing compile-fail type-state guards;
  - missing source pre-bind exact-state proof;
  - missing genuine broker-position Long/Short positives;
  - missing non-empty known-order/pending-request retention positives.

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
- genuine broker-position Long/Short fixtures carry real `target_open_positions`
  rows and restore through the full strict pipeline;
- non-empty known-order and pending-request indexes are retained through the
  restored receipt;
- open broker-position side mismatches block before callback through the common
  callback-zero retained-capability assertion helper.

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
stage5d_manifest_sha256: cc98123d6b39408b7a79042340a1ecef058611cfbe0d1ac3f535ee600c1025b9
stage5d_checker_sha256: 1a57390b3ab327f8ed1c6c2d528365633d898a86fce7ff4538b9001a1d01d8a2
stage5d_negative_harness_sha256: 7788a64e45706a1731bd32d8a3acb1de841de6d3caba49805a00be9813e97afa
forbidden_surface_scan_sha256: b2eb25d6dd54be86155df09f192e257c30083594fa505d3f7abfe252c3d49461
stage5c_paper_host_sha256: 19244c5e576f57ba092d39f08c933172a2d34dd5c4b7be9979e52fba65df9261
stage5d_persistence_sha256: b14520aff6a11012978ace86c53db5ca81d442d2538905cc9b10bc8ce8d0c1a2
review_gate_script_sha256: a7656932f6fed90a21d6c4bfae15f19b4a721fa0cfc7afd1e455017aa8632355
```

## Boundary statement

This slice only hardens the already opened no-I/O lifecycle transition. It does
not add Redis reads/writes, FINAM calls, transport, dispatch, runtime-live,
autonomous recovery workers or real order execution.
