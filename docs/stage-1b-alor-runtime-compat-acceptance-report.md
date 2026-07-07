# Stage 1B — ALOR runtime compatibility acceptance report

Status: ready for review.

Date: 2026-07-07.

## Review response summary

This patch treats the previous Stage 1 package as Stage 1A: a correct
foundation, but not a hard compatibility freeze. Stage 1B adds the missing
review-grade pieces:

- field-by-field runtime compatibility tables;
- Redis stream / consumer-group mapping;
- runtime source migration vs adapter ADR;
- ALOR runtime-state fixtures;
- seed-required policy;
- stronger runtime-state parity evidence script;
- no hardcoded live VPS IP in source example;
- no IMOEXF-specific riskgate profile hardcoded inside `broker-core`;
- JSON decimal parsing no longer uses `as_f64()` in ALOR oracle seed mapping.

## Safety boundary

Unchanged:

- continuous runtime-live disabled;
- `command-consumer-to-real-FINAM` disabled;
- strategy-driven real FINAM send disabled;
- Stop/SLTP/bracket/replace/multi-leg disabled;
- FINAM Runtime `LiveReady` disabled.

## Code changes

- `PaperHybridIntradayOracleSeed` now carries pending entry/exit, TP/SL id
  placeholders, MR prices, safe-mode fields, overnight exit date, and riskgate
  profile id.
- `PaperLedgerSnapshot` preserves those fields in
  `PaperHybridIntradayRuntimeStateProjection`.
- The seed bridge accepts `risk_gate_profile_id` from seed/config instead of
  hardcoding an IMOEXF profile in `broker-core`.
- `finam-paper-runtime-consume` supports:

```json
{
  "alor_oracle": {
    "seed_required": true,
    "missing_seed_policy": "BlockParityRun"
  }
}
```

When `seed_required=true`, missing/invalid seed blocks the parity run.

## Fixtures

Added synthetic fixtures under:

```text
tests/fixtures/alor_runtime_compat/
```

The fixtures cover:

- flat clean runtime state;
- non-flat runtime state;
- pending entry;
- pending exit;
- safe-mode/dirty-start state;
- riskgate state;
- expected flat clean seed shape.

## Evidence script

`scripts/m4_3x_runtime_state_parity_evidence.py` now emits schema v2 with:

- `source_commit`;
- `vps_host`;
- FINAM WS/runtime/DLQ stream names;
- ALOR runtime-state stream name;
- `XLEN`/`XPENDING` diagnostics;
- compared bar key/timestamp;
- OHLCV diagnostic deltas where available;
- safety flags;
- divergence classification;
- expected/waived/blocker divergence counts;
- final status:
  `Synchronized`, `ExpectedDivergenceOnly`, `BlockedDivergence`, `Unseeded`,
  `SafetyBoundaryOpen`, or `EvidenceIncomplete`.

Raw Redis payloads are not exported.

## Acceptance state

Stage 1B can be accepted only as a compatibility-contract freeze for paper/shadow
parity. It is not permission for runtime-live or real strategy-driven order
sends.

Recommended next stage after acceptance:

1. Full-session FINAM M10 vs ALOR M10 parity report.
2. Broker-truth bootstrap into runtime lifecycle.
3. Real hybrid BO/MR/riskgate semantics attached behind paper boundary.
4. Runtime command consumer in paper/mock ACK mode.
