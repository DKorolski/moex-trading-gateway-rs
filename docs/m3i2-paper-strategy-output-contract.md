# M3i-2 paper strategy decision/output contract

M3i-2 adds the strategy output side of the paper/shadow migration. It does not
open live trading and does not add a direct strategy-to-Redis, strategy-to-M3e,
or strategy-to-FINAM path.

The accepted route is:

```text
M3iStrategyPaperInput
-> M3iPaperStrategySignal
-> M3iPaperStrategyOutputCandidate
-> M3hRuntimeDryCommandCandidate
-> existing M3h dry command emitter
-> M3e command stream
```

The rejected routes are:

```text
Strategy -> BrokerCommand -> endpoint
Strategy -> Redis command stream directly
Strategy -> reqwest / FINAM DTO / FINAM endpoint
Strategy -> Stop/SLTP/bracket/replace/multi-leg
```

## Contract

- Strategy output is broker-neutral and FINAM DTO-free.
- `request_id` is deterministic from strategy id, decision entry, decision bar,
  and signal fields.
- The strategy-side pending mutation is staged before M3h emission.
- Duplicate `request_id` is ignored and cannot publish a second command.
- `NotEmitted` and publish failure both roll back the strategy-side pending
  mutation and are counted as dropped intents.
- A published dry command stays pending until a later dry ACK/reconciliation
  stage.

## Live boundary

M3i-2 keeps these flags closed:

- `runtime_live_attachment_allowed = false`
- `live_ready_allowed = false`
- `external_order_endpoint_allowed = false`
- `real_finam_order_endpoint_used = false`
- `stop_sltp_bracket_replace_multileg_allowed = false`

## Evidence

Use:

```bash
python3 scripts/m3i2_paper_strategy_output_contract_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```

The evidence report is written to:

```text
reports/m3i-paper-shadow/m3i2-paper-strategy-output-contract-evidence.json
```
