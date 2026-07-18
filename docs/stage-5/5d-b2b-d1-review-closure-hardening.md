# Stage 5D-b2b-d1-r6 — acceptance-evidence and blocker-uniformity closure hardening

Status: implementation candidate.

Stage 5D-b2b-d1-r6 closes the r5 acceptance/evidence findings for the
controlled runtime-state-restored transition without opening any operational
wiring.

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
- Stage 5D negative harness inventory expands from `66` through `82` to `93`
  cases with
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
  - missing r5 strict round-trip helper/proofs;
  - missing paper-only blocker proof;
  - missing blocker ownership table;
  - missing r6 strict Long/Short/known-order/pending-request markers;
  - bypassed common blocked helper;
  - missing quantity/expiry/timestamp/non-ack/identity/generation ownership.

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
  rows and restore after strict JSON serialization and
  `Stage5dPersistenceEnvelope::from_json_str_strict`;
- non-empty known-order and pending-request indexes are retained through the
  restored receipt after the same strict JSON round-trip;
- open broker-position side mismatches block before callback through the common
  callback-zero retained-capability assertion helper.
- explicit `is_paper_only == false` and non-acknowledged recovery decision
  blockers are covered by the common callback-zero assertion helper;
- `docs/stage-5/stage5d-b2bd1-r6-blocker-ownership.json` records
  machine-checked blocker ownership for every remaining mismatch, including
  cases owned before b2b-d.

## r6 logical coverage counts

- Positive restored cases: 7 logical paths: clean restore, exact expiry,
  exact bootstrap notification, strict Long, strict Short, strict known-order
  index, strict pending-request index.
- Representable blocker cases: 20 ownership rows, all routed through the common
  callback-zero retained-capability helper.
- Earlier-owned blocker cases: 8 ownership rows: strategy, account,
  instrument, config, profile, riskgate evidence, riskgate identity and
  riskgate generation.
- Defensive branch: 1 row for broker quantity nonrepresentability.
- Terminal cases: 4 post-callback terminal matrix categories plus the explicit
  callback-intent terminal path.
- Compile-fail cases: capability construction, consumed input, restored/injected
  type separation, blocked/terminal separation and private bridge/preflight
  imports.
- Negative-harness cases: 93 Stage 5D additive cases and 87 forbidden-surface
  marker-pinned cases.

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
stage5d_manifest_sha256: a89bb91aedf1e45f74ee3dd5372afe102c19f0f67c79f0a9372c7ee4b52308f7
stage5d_checker_sha256: 45053f0405abba67306929b851d07b0ef0948e1e17d8ead2b11d6502180e15fc
stage5d_negative_harness_sha256: 54a89ea15064dfc36e0da8f08f4126bdcfaf4db277c09a9907d0695d469dcdd1
forbidden_surface_scan_sha256: cae486ac6b83c2b386969d672ca644289fd85844bc317caaa96d3b43a6a6b01c
stage5c_paper_host_sha256: 791c29147f4172982086520e17ea96a4251265db0c576c4a2be2e2b8f1a86a12
stage5d_persistence_sha256: de3ca0742964522b45d2fcaefd2dc069bf42f2a88482df3c72e1bfb346f1d8fa
review_gate_script_sha256: a7656932f6fed90a21d6c4bfae15f19b4a721fa0cfc7afd1e455017aa8632355
```

## Boundary statement

This slice only hardens the already opened no-I/O lifecycle transition. It does
not add Redis reads/writes, FINAM calls, transport, dispatch, runtime-live,
autonomous recovery workers or real order execution.
