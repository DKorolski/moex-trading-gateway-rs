# Operational market-data parity — retained work, 2026-10-02

Owner reminder: WS bar subscriptions and the remaining operational checks must
not disappear behind historical sparse-M10/model replay. This is tracking in
the **existing O3/O4 and subsequent paper-session comparison**, not a new stage,
a redesign, permission to run a stand, or a claim that the checks passed.

Current boundary: sparse source correction in progress; O2 HOLD. Finish the
source package and its review, then existing artifact/install/explicit O2
permissions. O3 remains synthetic; O4 remains separately authorized read-only
FINAM data plus paper execution. Existing phase deadlines and roles stay intact.
Multi-session evidence cannot be inferred from a bounded O4 run.

## Required observations before operational parity / live-micro consideration

| Area | Minimum useful proof | When / disposition |
| --- | --- | --- |
| WS subscription | Exact configured endpoint/path, instrument `IMOEXF@RTSX`, M1, subscription key/response or rejection, and actual final bar receipt. Socket connected alone is not subscribed/ready. No tokens in logs. | Review local wire/error cases while preparing O4; verify actual subscription during separately authorized O4. |
| WS continuity | Heartbeat/silence vs actual bar freshness; bounded reconnect and resubscribe; expired-token/error handling; no duplicate active subscription after reconnect. | One controlled interruption in the paper window; preserve accepted high-water. No open-ended fault campaign. |
| M1 finality and overlap | Repeated/provisional updates cannot become multiple final candles; out-of-order/equal duplicate/conflicting updates; REST/live overlap cannot run a second strategy callback. | Reuse existing tests and retained source IDs, then one operational witness. Conflicts stop advancement. |
| Sparse coverage | Silence is not proof of an empty minute. Closed-range REST admission confirms a sparse bucket; an already confirmed range needs no mandatory second GET. Empty M10 and failed confirmation remain fail-closed. | Current source correction implements the contract; O4 verifies its bounded read-only use. No generic polling scheduler. |
| Time/session boundaries | UTC/MSK, candle-start strategy label vs close-bound canonical ID, market breaks/weekends, first session bar, final M10/EOD without waiting for the next bar. | Check on a normal session and an EOD observation. Calendar absence and a broken subscription are distinct. |
| End-to-end delivery | Record nominal open/close, provider/receive/publish/admit/commit times; distinguish stale history from fresh live data; inspect M10/command lag and PEL. | Use accepted thresholds/phase configuration. No green health solely from a connected socket or old bars. |
| Durable paper lifecycle | Actual M10 → strategy intents → paper outcome → ACK/truth → source XACK-last; bounded restart preserves identity and does not duplicate commands/fills/callbacks. | Reuse accepted lifecycle mechanisms; operational paper evidence after source/integration acceptance. FINAM execution remains absent. |
| ALOR operational comparison | Same symbol/model/config, warmup dates and session coverage; compare anchors, pending IDs/state transitions, intent times and paper trades. Keep input-price differences and publication delays visible. | Several separately approved paper sessions, not historical OHLC equality alone. ALOR remains an oracle, never production fallback. |
| Safety/stop | Phase deadline, clean stop, no new work after stop, retained history/receipts, P0/DB0 isolation, read-only token. | Existing accepted operator gates; do not broaden authority to order POST/DELETE, broker dispatch or runtime-live. |

## Existing surfaces to reuse

- `crates/broker-finam/src/ws.rs`: subscription wire, envelope mapping, final-bar
  mapping tests. Existing tests do not prove live subscription delivery.
- `crates/broker-cli/src/main.rs`: existing `finam-ws-shadow-loop`, reconnect /
  resubscribe and bar-freshness diagnostics. P0 evidence is historical and does
  not establish the future P1 O4 feeder's readiness.
- `crates/finam-gateway/src/stage8b_p1f_fixed_producers.rs`: separated O3/O4 roles
  and retained Prepared/Published high-water.
- Existing bounded GET client / new closed-range evidence, current schedule
  publisher and durable semantic/ACK/truth/XACK consumers. Do not replace them
  with a second recovery or lifecycle framework.

The exact O4 endpoint/subscription behavior must be rechecked against the
then-current provider contract and installed build at that boundary. No live
WS, token refresh, reconnect or EOD witness was collected by this local change.

## Evidence distinction

The accepted diagnostic proves historical data comparison. The new local
four-day replay proves equal model decisions under one paper-fill assumption.
Neither proves delivery latency, WS resubscription, ACK timing or operational
EOD. Retain the observed next-day ALOR 23:40 publication delays as an oracle
limitation; do not normalize them away or claim timely EOD from nominal bars.

Before operational sparse use obtain the agreed FINAM clarification on
no-trade-minute candles, closed-range finality and the exact end boundary.
This remains separate from implementing/testing local source code.
