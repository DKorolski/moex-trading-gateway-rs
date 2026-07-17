# Stage 5D-b2b-d1 — runtime-restored review-closure hardening

Status: implementation candidate.

Stage 5D-b2b-d1 closes the review findings from the first controlled
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
- Stage 5D negative harness inventory expands from `49` to `54` cases with
  marker-pinned checks for:
  - missing runtime intent guard;
  - guard moved after `debug_assert!`;
  - missing exact post-callback broker-truth guard;
  - missing bootstrap-notification timestamp guard;
  - missing exact flat-side guard.

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

## Gate evidence

```text
review_gate_log: reports/stage5d/stage5d-b2bd1-review-gate.log
review_gate_log_lines: 1590
review_gate_log_sha256: 7fc873c131e1fb8a76ba3ad34d631dddde01a2b9dfc80071212d0a857bcc47b2
```

The full gate ended with:

```text
stage5d-b2bd1-review-gate: all required gates passed
```

Pinned hashes:

```text
stage5d_manifest_sha256: 0bb3dc0aa972b9dfeb7357350e9e5a108e9955052a2c5c4977eec69c87a07704
stage5d_checker_sha256: e535ea844e85f9e0a98d656b14bd90c1c9b329ad51a2c97bbdcfa7d89df44a08
stage5d_negative_harness_sha256: 8e488ead365defa64e9eed6f31f5599f240d844ccdafa5b476868dd2c1191a49
forbidden_surface_scan_sha256: d48ccd745d68f96497019a3b1ed445727b4ca72a63df26639cb24f848231a0c5
stage5c_paper_host_sha256: dd7018481cf1de9df73b5c452caadabf5da823c658ab2d8a5583e021a66f44df
stage5d_persistence_sha256: 7c2d1654360f4dfd2ed7e3f304334b0aaff233d3c4c8557b2caeb7629e0a8c17
review_gate_script_sha256: a7656932f6fed90a21d6c4bfae15f19b4a721fa0cfc7afd1e455017aa8632355
```

## Boundary statement

This slice only hardens the already opened no-I/O lifecycle transition. It does
not add Redis reads/writes, FINAM calls, transport, dispatch, runtime-live,
autonomous recovery workers or real order execution.
