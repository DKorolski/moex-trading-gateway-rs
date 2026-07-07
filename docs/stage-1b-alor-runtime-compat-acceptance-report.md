# Stage 1B — ALOR runtime compatibility acceptance report

Status: accepted.

Date: 2026-07-07.

## Review response summary

Review accepted Stage 1B as the compatibility-contract freeze for IMOEXF
`HybridIntradayRuntime` paper/shadow parity and allowed Stage 2A design/prep
runtime source migration. Runtime-live and strategy-driven real FINAM sends
remain forbidden.

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
- ADR accepted status for Stage 2A planning;
- explicit Stage 1B scope: IMOEXF `HybridIntradayRuntime` only;
- exact ALOR field coverage ledger with preserve/map/unsupported policies;
- evidence script no-Redis robustness: missing CLI produces
  `EvidenceIncomplete` instead of crashing;
- automated fixture assertions for every Stage 1B fixture.

## Safety boundary

Unchanged:

- continuous runtime-live disabled;
- `command-consumer-to-real-FINAM` disabled;
- strategy-driven real FINAM send disabled;
- Stop/SLTP/bracket/replace/multi-leg disabled;
- FINAM Runtime `LiveReady` disabled.

## Code changes

- `PaperHybridIntradayOracleSeed` now carries pending entry/exit, deferred
  entry/exit markers, position-adoption markers, dirty-start/manual-intervention
  markers, TP/SL id placeholders, MR prices, safe-mode fields, overnight exit
  date, and riskgate profile id.
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
- deferred exit;
- safe-mode/dirty-start state;
- riskgate state;
- expected flat clean seed shape.

All Stage 1B fixtures are connected to automated assertions in `broker-cli`
tests.

## Evidence script

`scripts/m4_3x_runtime_state_parity_evidence.py` now emits schema v2 with:

- `source_commit`;
- `vps_host`;
- FINAM WS/runtime/DLQ stream names;
- ALOR runtime-state stream name;
- `XLEN`/`XPENDING` diagnostics;
- consumer-group lag via `XINFO GROUPS` where Redis provides it;
- compared bar key/timestamp;
- OHLCV diagnostic deltas where available;
- safety flags;
- divergence classification;
- expected/waived/blocker divergence counts;
- final status:
  `Synchronized`, `ExpectedDivergenceOnly`, `BlockedDivergence`, `Unseeded`,
  `SafetyBoundaryOpen`, or `EvidenceIncomplete`.

Raw Redis payloads are not exported.

No-Redis smoke:

```bash
bash scripts/test_m4_3x_evidence_no_redis.sh
```

Expected result: exit code `0`, evidence status `EvidenceIncomplete`,
`raw_payload_exported=false`.

## Acceptance state

Stage 1B can be accepted only as a compatibility-contract freeze for paper/shadow
parity. It is not permission for runtime-live or real strategy-driven order
sends.

Recommended next stage after acceptance:

1. Stage 2A design/prep only: runtime source migration inventory and plan.
2. Full-session FINAM M10 vs ALOR M10 parity report.
3. Broker-truth bootstrap design for runtime lifecycle.
4. Real hybrid BO/MR/riskgate semantics design behind paper boundary.
5. Runtime command consumer design in paper/mock ACK mode.

Stage 2B implementation remains blocked until the exact field ledger gaps are
resolved by structured mapping, accepted waiver, or explicit out-of-scope proof.
