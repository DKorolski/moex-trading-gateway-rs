# M3i-4 paper/shadow replay and dry ACK application

M3i-4 connects the accepted M3i paper strategy output path to the accepted M3h
dry command emitter and M3e dry ACK lifecycle, then applies the ACK result back
to paper strategy state.

This is still a paper/shadow package. It does not enable live trading.

## End-to-end route

```text
LiveFinal bar fixture
-> M3h StrategyDecisionTick
-> M3iStrategyPaperInput
-> M3iPaperStrategySignal
-> M3iPaperStrategyOutputCandidate
-> M3hRuntimeDryCommandCandidate
-> M3h dry emitter
-> M3e dry command consumer
-> M3e dry ACK
-> M3i paper strategy state
```

The strategy still cannot publish directly to Redis/M3e and cannot call any
FINAM order endpoint.

## ACK application matrix

M3i-4 handles:

- `Rejected + DryRunOnly` -> paper acknowledged, pending removed;
- `Duplicate + DuplicateCommand` -> duplicate acknowledged, no second pending;
- `Expired + ExpiredCommand` -> dropped terminal;
- `Rejected/Error + local reject style reason` -> dropped terminal;
- missing/stale ACK -> pending remains visible in operator report;
- unknown ACK shape -> manual policy required.

## TIF scope

The current M3i paper strategy policy intentionally allows `TimeInForce::Day`
only. IOC/FOK/GTC require a separate explicit policy matrix before any
live-micro discussion.

## Boundary

Still forbidden:

- `LiveReady`;
- runtime live attachment;
- external FINAM POST/DELETE;
- non-loopback order endpoint;
- direct strategy publish to Redis/M3e;
- command-consumer-to-real-FINAM transport;
- Stop/SLTP/bracket/replace/multi-leg.

## Evidence

Use:

```bash
python3 scripts/m3i4_paper_shadow_replay_ack_application_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

The evidence report is written to:

```text
reports/m3i-paper-shadow/m3i4-paper-shadow-replay-ack-application-evidence.json
```
