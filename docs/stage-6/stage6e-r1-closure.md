# Stage 6E-R1 — durable-chain closure repair

Stage 6E-R1 is a direct, bounded repair of Stage 6E candidate
`ec71791563a933889eb825f6f8f0846915ba6415`. It opens no Redis production
consumer, FINAM POST/DELETE, broker dispatch, runtime-live or real-order path.

## Request-scoped fresh truth

The production issuer requires an explicit `StrategyRequestId`. The selected
request must be present in both the current Stage 5 lifecycle projection and
the Stage 6 replay, and must be a member of the active semantic cross-binding.
This supports multiple simultaneous current Place/Cancel lifecycles without a
single-match assumption. Finalized historical requests cannot be selected.

The opaque accepted capability remains linear, non-cloneable and
non-serializable. It binds the selected request, replay, journal frontier,
authenticated checkpoint, semantic cross-binding and current restore epoch.

## Current-process restore epoch

Only a successful authenticated restart creates the process-local restore
epoch. Its generation is produced from host process identity, host time and a
monotonic process counter; it is never decoded from prior persistence or
accepted from broker input.

Fresh truth must prove:

```text
current restore completion
  < local collection start
  <= section local observations / row local receipts
  <= local collection completion
  <= trusted host validation observation
```

Broker/source timestamps remain diagnostic event chronology and may predate a
restart. They cannot manufacture post-restore collection authority.

Integration fingerprint schema v3 binds the restore-epoch fingerprint. A new
process restart creates a new generation and rejects a capability issued by a
prior generation even when replay and checkpoint bytes are otherwise equal.

## Governance and next gate

`docs/current-status.md`, `docs/roadmap.md` and reviewer onboarding identify
6E-R1 as the current candidate. Stage 7 remains closed until independent
acceptance. The R1 gate checks these canonical top-level markers and fails on
status drift.

Primary gate:

```bash
bash scripts/stage6e_r1_gate.sh
```
