# M3h-5 shadow replay and readiness report package

M3h-5 closes the runtime-shadow stage with a redacted replay/readiness report.
It proves the dry end-to-end path while keeping all live boundaries closed:

```text
LiveFinal bar
-> StrategyDecisionTick
-> dry BrokerCommand
-> M3e command stream
-> M3e dry ACK
```

Report coverage:

- decision tick count;
- dry command published count;
- duplicate request-id count;
- `NotEmitted` / dropped intent count;
- pending emission count;
- oldest pending emission age;
- redacted pending request hashes;
- maybe-published-not-finalized count;
- not-ready suppressions;
- inbound `LiveReady` blocked count;
- duplicate and non-monotonic bar counts;
- M3e dry ACK count;
- M3e duplicate ACK count.

Operator visibility:

- `PendingEmission` is surfaced with count, oldest age, and request hash list;
- `NotEmitted` remains visible as dropped-intent / rollback evidence;
- raw request IDs and raw payloads are not exported.

Safety boundary:

- no runtime live attachment;
- no `LiveReady`;
- no external FINAM `POST / DELETE`;
- no direct endpoint stream;
- no command-consumer-to-real-FINAM transport;
- no stop/SLTP/bracket/replace/multi-leg.

M3h-5a hardens closure accounting. `m3h_runtime_shadow_stage_closed = true`
only when this closure-grade predicate holds:

- `end_to_end_dry_path_to_m3e_ack = true`;
- `decision_tick_accounting_ok = true`;
- `dry_command_report_redacted = true`;
- `pending_emission_operator_visibility_ok = true`;
- `m3e_command_stream_only = true`;
- closed live boundaries remain true.

Decision tick accounting is:

```text
decision_tick_count ==
  dry_command_published_count
  + not_emitted_count
  + duplicate_request_id_count
```

Diagnostics-only reports remain useful, but they must not mark the M3h stage
closed without an end-to-end M3e dry ACK proof.

Evidence:

```bash
python3 scripts/m3h5_shadow_replay_readiness_report_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
