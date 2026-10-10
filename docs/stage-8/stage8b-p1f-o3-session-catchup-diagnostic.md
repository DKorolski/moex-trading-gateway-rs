# O3 session catch-up: local draft and admission blocker

2026-10-09. Base: `2be020c234a01f8627d9f7621623dc603c912e23`.
**IMPLEMENTATION INCOMPLETE / O3 NOT_RUN / NOT FOR INSTALLATION.**
This is a programmer's diagnostic, not an independent acceptance.

## Accepted boundary

Config-only source and installation evidence are ACCEPTED by
`REVIEW_2be020c_O3_CONFIG_INSTALLATION_EVIDENCE_ACCEPT_20261009_RU.txt`.
The installed config SHA is
`3a031f24ac91990bb4b00308fad93d48b36af09d135ab5d4ecc62e4702e5c3e8`.
O2 remains COMPLETED/1/26; no O2 rebootstrap or root reset is authorized.
This work has made no SSH, VPS, signing, FINAM, DB0, GitHub or systemd calls.
Redis tests use disposable local servers only.

## What was implemented locally

- An in-memory observation-only catch-up prototype for the initial, flat,
  no-effect P1 Ready seed. No position, pending order, deferred entry/exit,
  acquired owner or previous trading cycle is allowed.
- It reuses the first-boot observation kernel, not generate-and-discard.
  All required bars are checked before mutation. Existing sparse-M1 aggregation,
  opening strategy labels, close-bound durable identity and model parameters
  are retained. Previous-session tail is included to update yesterday's levels.
- The admitted full observed receipt travels with the canonical V2 input.
  Its digest was already part of the canonical source binding; no new journal
  format, root, reset, secret or clock override has been introduced.
- The prototype feeds the ordinary semantic transaction. It does not separately
  persist warmed state. This path is deliberately NOT claimed to have passed a
  positive long-gap transaction/restart test.

The pure synthetic test uses the actual O2 seed's public scalar values and the
unchanged profile fingerprint
`665f18112142f6c9aee1b613e341870f0d559d783fe41df725e26415507e6290`.
It demonstrates previous-tail/current-day warmup without historical intents,
then an ordinary breakout decision. It is not a real-broker replay or a
cryptographic restoration of the operational root.

Negative cases: empty required M10, truncated receipt, invalid tick, conflicting
last close, open position, pending request, excessive gap, reversed chronology.
The four-day cap and strict model-session grid are provisional fail-closed
bounds, not a newly accepted market-calendar contract. Empty clearing/holiday
intervals need explicit existing schedule/coverage evidence, not invented bars
or an unconditional skip. Source lookback must cover the retained tail; the
patch does not silently extend a signed source plan.

## Reproduced blocker (not a warmup success)

The initial attempt at positive Redis continuation and journal-ahead replay
failed before RequestAccepted/S1 with:

```text
Stage5c(TimerContinuation(NextBar(Semantic(BrokerTruthExpired))))
Runtime(DurableOrderingViolation)
```

The final regression names this refusal explicitly:
`observed_session_catchup_preserves_expired_admission_refusal_and_seed`.
It asserts no provider action, command publication or XACK, unchanged journal
and seal, one retained PEL entry, and restart back to Ready. A green result here
means the safety refusal is preserved, NOT that positive O3 passes.
The two original positive adjacent-bar/restart tests are unchanged.

Public seal retained in the prior read-only preflight gives:

| continuation context | UTC | Moscow |
|---|---|---|
| checked / issued | 2026-10-08 17:04:03.681 | 2026-10-08 20:04:03.681 |
| expires | 2026-10-08 17:09:03.681 | 2026-10-08 20:09:03.681 |

These are historical public observations, not a fresh operational observation.
No HMAC key was read and the diagnostic does not claim independent HMAC
verification. The previous diagnostic ZIP binds the original observation:
`moex-trading-project-2be020c-o3-seed-preflight-20261009.zip`,
SHA-256 `675fbba6edbe77b69450660be4702b88df63c200bae7a733fe7deb6e8e01f807`.

Code: `apply_stage5c_semantic_bar_at_with_replay_boundary` rejects expired
admission before the fresh callback. The saved continuation context is restored
from the accepted seal. O3's fresh Stage4 report is currently consumed by the
schedule publisher, not joined to this runtime admission. Increasing its TTL,
altering old checked/expiry timestamps or weakening the rejection is not a fix.
Existing adjacent-bar tests bootstrap just before the next close; they do not
prove continuation from yesterday's stopped O2 seed.

## Narrow decision needed before completing the source patch

Authorize/review a **paper-only re-admission contract** for the authenticated
initial flat seed, alongside the already requested historical observation:

1. Bind a fresh checked paper truth/admission to the exact retained seed,
   operational identity, account/instrument, profile and fresh O3 phase.
   A signature proving schedule eligibility alone must not silently become
   proof of order/position state. Specify the evidence constructor explicitly.
2. Retain the exact admission evidence required for deterministic journal-ahead
   replay. A restart must not obtain a different decision by using a later
   timestamp or reconstructing an unrecorded observation.
3. Feed only the explicitly covered missing session bars through observation;
   preserve calendar breaks/holidays and reject unproven empty intervals.
4. Positive tests: actual old-seed shape → next-session observation → fresh
   decision → existing paper Market/ACK/truth/XACK; compare an uninterrupted run
   with pre-append, RequestAccepted-before-S1 and post-S1 restarts.
5. Negative tests: expired/foreign/missing admission, non-flat seed, unknown
   gap, source drift; no historical orders, duplicate effects or early XACK.

Reuse the existing Stage4/Stage5C admission and Stage6/7 commit mechanisms where
possible; do not add a generic recovery framework. No broker dispatch, FINAM
POST/DELETE, runtime-live or DB0 activation. Until this contract is settled the
source remains an uncommitted draft, authority pins are not regenerated, and
no release/installation or bounded O3 is prepared.

After the completed source receives review: one release/authority package,
separately accepted stopped installation, then authorized bounded O3.
O4 WS bars/subscriptions, reconnect, REST overlap/freshness, EOD timing and
multi-session ALOR paper parity remain planned; this diagnostic does not close
any of those items.
