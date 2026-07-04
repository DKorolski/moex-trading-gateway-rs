# M3i-3 paper strategy replay and state restore package

M3i-3 closes the P1 hardening items raised after M3i-2 while keeping the stage
paper/shadow-only.

## Added scope

- deterministic request id hardening;
- local strategy-output shape validation;
- JSON-backed paper strategy state restore;
- replay/restore evidence for pending, published-pending-ACK, dropped, duplicate,
  and suppressed outputs.

## Request identity

The hardened request fingerprint explicitly includes:

```text
strategy_id
strategy_version
strategy_params_hash
account_id
instrument symbol / venue symbol
timeframe
decision entry id
decision bar key
signal shape
```

This prevents account/instrument/version collisions during paper replay and later
strategy migration.

## Local shape validation

M3i rejects invalid paper strategy outputs before they can be converted into an
M3h dry command candidate:

- Market order with `limit_price`;
- Limit order without `limit_price`;
- `qty <= 0`;
- unsupported TIF;
- Stop/SLTP/bracket/replace/multi-leg shapes.

## State restore

The JSON-backed state store proves restart behavior for:

- pending staged before emission;
- published but still pending dry ACK;
- NotEmitted / PublishFailed dropped intent;
- duplicate request id after restart.

Dropped intents remain terminal and are exported only as redacted request hashes.

## Boundary

M3i-3 still does not enable:

- `LiveReady`;
- runtime live attachment;
- external FINAM POST/DELETE;
- non-loopback order endpoint;
- direct strategy publish to M3e/Redis;
- Stop/SLTP/bracket/replace/multi-leg.

The only onward route remains:

```text
M3i paper output -> M3hRuntimeDryCommandCandidate -> M3h dry emitter -> M3e command stream
```

## Evidence

Use:

```bash
python3 scripts/m3i3_paper_strategy_replay_state_restore_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

The evidence report is written to:

```text
reports/m3i-paper-shadow/m3i3-paper-strategy-replay-state-restore-evidence.json
```
