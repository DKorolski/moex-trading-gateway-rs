# M3f-4a runner idempotency and M3f closure

M3f-4a hardens the M3f reconciliation runner after the accepted M3f-4
runner-core review.

Closure guarantees:

- partial-apply restart is idempotent;
- terminal/recovered local records are ignored on rerun and are not reported as
  orphan broker orders when the same broker-truth snapshot is replayed;
- manual-intervention records stay stable and keep readiness blocked;
- redacted JSON report sink writes an operational artifact;
- blocker summary includes counts by kind, affected request count, oldest
  pending age, and manual intervention count.

The M3f broker-truth reconciliation stage remains read-only. It does not enable
real FINAM order endpoints, runtime live attachment, `LiveReady`, or
stop/SLTP/bracket functionality.

Evidence:

```bash
python3 scripts/m3f4a_reconciliation_closure_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
