# Stage 3F — market-data parity acceptance report

Status: implemented for review.

Date: 2026-07-09.

## 1. Scope Stage 3

Stage 3 closes the market-data parity contract to strategy-input level. Its
scope is deliberately narrow:

- ALOR native M10 remains the strategy-facing oracle;
- FINAM final M1 bars are the accepted FINAM source;
- FINAM final M1 bars may become strategy candidates only through strict
  canonical M1-to-M10 aggregation;
- raw M1 bars are diagnostic/aggregation inputs only;
- candidate strategy bars require gap/recovery/session proof before publication;
- evidence artifacts and reports are redacted/source-bound.

Stage 3 is still a paper/shadow/evidence stage. It does not attach the real
runtime-live strategy loop and does not authorize real FINAM order routing.

## 2. Accepted patch list

| Patch | Commit | Status | Closure |
| --- | --- | --- | --- |
| Stage 3A | `6755998` | accepted | Market-data parity plan and evidence schema foundation. |
| Stage 3B | `86356d5` | accepted | Source-only ALOR-native-M10 vs FINAM-derived-M10 comparator foundation. |
| Stage 3B-1 | `4b0cc4b` | accepted | Strict derivation hardening; blocked candidates do not become model bars. |
| Stage 3C | `135a846` | accepted | Redacted multi-bucket parity report generator. |
| Stage 3C-1 | `7e7c162` | accepted | Duplicate bucket hardening; no silent overwrite. |
| Stage 3D | `79bfb7d` | accepted | Controlled offline active-session evidence collector foundation. |
| Stage 3D-1 | `1d69f62` | accepted | Recovery/session/input-gate hardening foundation. |
| Stage 3D-2 | `c746460` | accepted | Recovery/session consistency hardening. |
| Stage 3D-3 | `b84a5ef` | accepted | Controlled operator-run input adapter for approved/redacted sources. |
| Stage 3D-3a | `8ae85f0` | accepted | Approved source schema v2 and `session_window_utc` hardening. |
| Stage 3E | `05d31ec` | accepted as foundation | Reconnect/gap recovery evidence contract and redacted writer. |
| Stage 3E-1 | `cba40be` | accepted as foundation | Recovery report consistency and publication counter hardening. |
| Stage 3E-2 | `1eebce1` | accepted | Replay-window evidence completeness hardening. |
| Stage 3E-3 | `68591fc` | accepted / closed | Final replay watermark/mode consistency hardening. |
| Stage 3F | current | implemented for review | Stage 3 acceptance report and Stage 4 entry boundary. |

## 3. What Stage 3 proves

Stage 3 proves the market-data contract needed before broker-truth/bootstrap
work can continue:

- ALOR native M10 remains the oracle for the existing strategy-facing bar
  contract.
- FINAM native M10 remains blocked until separately characterized and accepted.
- FINAM final M1 -> strict canonical M1-to-M10 is the only accepted FINAM
  candidate path.
- Raw M1 cannot become a strategy/model bar.
- Non-M1 FINAM inputs cannot build Stage 3 FINAM M10 strategy candidates.
- Incomplete M1 buckets cannot become M10 strategy candidates.
- Duplicate exact source bars are idempotent.
- Conflicting duplicate buckets cannot silently overwrite evidence.
- Multi-bucket reports are redacted and source-bound.
- Approved input adapter requires source schema v2 and explicit
  `session_window_utc`.
- Approved ALOR/FINAM input bars outside the approved session window are
  rejected before evidence acceptance.
- Recovery evidence requires replay/gap proof before strategy publication.
- Replay/recovery bars cannot be counted as strategy/model bars.
- Overlap replay cannot create duplicate model bars.
- Entry is blocked while gap proof is missing.
- Exit/Cancel/Repair are not falsely blocked as Entry by the gap guard.
- RecoveryComplete requires:
  - M10 strategy timeframe;
  - timestamps inside the approved session window;
  - explicit replay window fields;
  - positive replay bar count;
  - valid replay ordering;
  - replay window covering the previous strategy watermark;
  - first fresh live final strictly after replay;
  - mode/attempt consistency;
  - `checked_ts` not earlier than the observed first fresh live final.

## 4. What Stage 3 does not prove

Stage 3 does not prove:

- broker-truth bootstrap into runtime;
- real strategy invocation;
- command consumer behavior;
- real FINAM execution;
- continuous runtime-live;
- strategy-driven real orders;
- FINAM Runtime `LiveReady`;
- real order/trade/position reconciliation at ALOR operational maturity;
- Stop/SLTP/bracket/replace/multi-leg behavior;
- RI/RTS migration;
- USDRUBF migration.

These remain future macro-stage gates.

## 5. Evidence and report contracts

Stage 3 evidence is governed by these source documents:

- [`stage-3-market-data-parity-plan.md`](stage-3-market-data-parity-plan.md)
- [`stage-3/3a-market-data-parity-evidence-schema.md`](stage-3/3a-market-data-parity-evidence-schema.md)
- [`stage-3/3b-market-data-parity-comparator-contract.md`](stage-3/3b-market-data-parity-comparator-contract.md)
- [`stage-3/3c-redacted-market-data-parity-report-generator.md`](stage-3/3c-redacted-market-data-parity-report-generator.md)
- [`stage-3/3d-controlled-active-session-evidence-collection.md`](stage-3/3d-controlled-active-session-evidence-collection.md)
- [`stage-3/3e-reconnect-gap-recovery-evidence.md`](stage-3/3e-reconnect-gap-recovery-evidence.md)

Clean source handoff archives must not include generated `reports/`, raw Redis
payloads, broker payloads, account ids, tokens, secrets, logs, `.env`, `.git`,
`target`, or `tmp`.

Expected local gates for the Stage 3F package:

```bash
cargo fmt
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/forbidden_surface_scan.sh
bash scripts/forbidden_surface_negative_harness.sh
bash scripts/order_endpoint_scanner_transition_spec.sh
python3 -m py_compile scripts/*.py
```

## 6. Remaining caveats

Stage 3 is a market-data parity and evidence-boundary closure, not a production
cutover.

Known caveats that carry into Stage 4:

- broker-truth snapshots exist, but broker truth is not yet mandatory runtime
  bootstrap input;
- the real old ALOR `HybridIntradayRuntime` source is not yet fully attached as
  a live FINAM runtime in this repo;
- paper runtime state projection has ALOR-compatible fields, but it is not yet
  the full ALOR hybrid BO/MR orchestrator;
- riskgate state can be seeded/projected, but true riskgate ledger integration
  remains future work;
- Stage 3 evidence is report/source-bound and does not by itself authorize
  runtime-live or real orders.

## 7. Why Stage 4 can start

After reviewer acceptance of this Stage 3F package, Stage 4 can start because
Stage 3 will have closed the strategy-input market-data contract that
broker-truth bootstrap needs to rely on:

- the strategy-facing bar timeframe is fixed at M10;
- the accepted FINAM path is final M1 -> canonical M10;
- raw M1 and non-M1 accidental strategy input are blocked;
- session-window and recovery proof requirements are explicit;
- reconnect/gap recovery cannot resume strategy publication blindly;
- Entry remains blocked while gap proof is missing;
- Exit/Cancel/Repair remain conceptually separate from Entry gap blocking;
- safety scanners still keep real order surfaces closed.

The next macro-stage may therefore focus on broker-truth bootstrap into runtime
lifecycle without reopening the market-data parity foundation.

## 8. Why runtime-live remains blocked

Runtime-live remains blocked because Stage 3 does not prove the runtime
lifecycle gates needed for continuous trading:

- broker-truth bootstrap is not wired as mandatory runtime input;
- runtime state restore/adoption semantics are not accepted for FINAM;
- real strategy invocation is not attached behind a paper boundary;
- runtime command consumer is not proven in paper/mock ACK mode;
- durable request/client/broker id chain is not accepted for runtime use;
- orders/trades/positions reconciliation loop is not ALOR-level mature;
- Stop/SLTP/bracket/replace behavior is not implemented or accepted.

## 9. Why real FINAM command consumer remains blocked

The real FINAM command consumer remains blocked because Stage 3 did not prove:

- broker-truth bootstrap before command acceptance;
- runtime command intake under paper/mock ACK parity;
- idempotent runtime request -> client order -> broker order chain;
- reconciliation after ambiguous ACK/place/cancel outcomes;
- account/instrument scoped order and position truth at runtime boundary;
- protective close/cancel/repair policies under real failures.

Stage 3F is not an implementation gate for real order routing.

## 10. Required gates before Stage 4 acceptance

Stage 4 should prove broker-truth bootstrap into runtime lifecycle. At minimum,
Stage 4 acceptance should require:

- `LoadBrokerTruthSnapshot` design/implementation behind paper boundary;
- `LoadRuntimeState` restore/adoption policy;
- bootstrap snapshot notification and redacted evidence;
- runtime state restored notification;
- warmup history and recovery of pending market-data streams;
- instrument-scoped positions and active orders as runtime truth inputs;
- readiness blockers for unknown/incomplete broker truth;
- no real command consumer attachment;
- no strategy-driven live order sends.

Stage 4 may prepare runtime lifecycle inputs. It must not enable real FINAM
execution by default.

## 11. Still forbidden

Still explicitly forbidden after this Stage 3F package:

- runtime-live;
- real FINAM command consumer;
- strategy-driven real FINAM orders;
- real FINAM `POST`/`DELETE` from runtime;
- Stop/SLTP/bracket/replace/multi-leg live behavior;
- RI/RTS migration;
- USDRUBF migration;
- `i64` surrogate adapter without a new ADR;
- changing BO/MR trading logic under the name of market-data parity.

## 12. Recommended decision

After reviewer acceptance of this Stage 3F package:

- Stage 3 can be marked accepted/closed as market-data parity to
  strategy-input level.
- Stage 4 can start as broker-truth bootstrap into runtime lifecycle.
- Runtime-live and real FINAM command consumer remain blocked until their own
  later macro-stage gates are accepted.
