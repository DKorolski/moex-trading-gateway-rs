# M3h-2a runtime bar ordering hardening

M3h-2a closes the conditional M3h-2 review gap around runtime shadow bar
ordering. The consumer now keeps an in-memory decision watermark per
`instrument|timeframe` and refuses to create a strategy decision tick for any
unique `LiveFinal` bar whose `open_ts` is older than or equal to the last
decision bar for that same key.

Behavior:

- duplicate bars are still handled first as `DuplicateBar`;
- unique non-monotonic `LiveFinal` bars become `NonDecisionBar` with
  `NonMonotonicLiveFinal`;
- `LiveUpdating`, historical/read-only, recovery, and unknown-source bars become
  `NonDecisionBar` with `NonFinalOrNonLiveSource`;
- non-live/non-final bars do not advance the decision watermark;
- instrument and timeframe watermarks are independent;
- restart policy is explicit:
  `EphemeralRequiresReplayOrBootstrap`.

Safety boundary:

- no `BrokerCommand` emission;
- runtime live attachment remains disabled;
- inbound `LiveReady` remains blocked;
- real FINAM order endpoints remain disabled;
- external order endpoint calls remain forbidden.

Evidence:

```bash
python3 scripts/m3h2a_runtime_bar_ordering_evidence.py \
  --source-archive reports/handoff/moex-trading-project-<commit>.zip
```
