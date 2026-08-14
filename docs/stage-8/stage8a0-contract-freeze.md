# Stage 8A-0 — FINAM contract refresh/freeze

Status: candidate; independent acceptance pending.

This slice refreshes and freezes the public FINAM order and instrument
prerequisite contracts before any Stage 8 production implementation. It is
limited to documentation, evidence and checkers. It does not authorize or add
FINAM order calls, broker dispatch, runtime-live or real orders.

## Contract decision

The official contract retrieved on `2026-08-14T16:40:58Z` matches the existing
project boundary for the initial Stage 8 subset:

- PLACE uses `POST /v1/accounts/{account_id}/orders`;
- CANCEL uses `DELETE /v1/accounts/{account_id}/orders/{order_id}`;
- initial PLACE is MARKET/LIMIT with `TIME_IN_FORCE_DAY` only;
- `client_order_id` is always the exact non-empty durable id and has at most 20
  characters, even though FINAM documents omission and broker generation;
- outgoing `comment` follows `Disabled/None` policy;
- STOP, STOP_LIMIT, MULTI_LEG, ValidBefore/GTD and protective behavior remain
  outside the initial execution scope.

The full official enum surfaces remain preserved in the normalized snapshot.
Narrower project policy is not represented as a narrower broker contract.

## Outcomes carried forward

PLACE and CANCEL success is only an accepted broker observation. Ambiguous,
transient and malformed outcomes require reconciliation and never blind retry.
For CANCEL specifically, documented 400 (already executed), 404 and defensive
undocumented 409/410 require reconciliation. A 401 blocks auth/readiness,
disarms execution, keeps the target unresolved and requires fresh read-only
truth.

After `DispatchAttemptRecorded` and send-capability consumption, a durable
request has no second execution attempt. `DefinitelyNotSent` does not permit
resending the same durable request; any future attempt requires a new durable
request, ClientOrderId, operator arm and capability.

## Instrument provenance

The future preflight must use current asset, account-specific asset params and
schedule data. Symbol, tick, lot, multiplier/step value and schedule sources are
hashed in the parity evidence. FINAM's public REST contract has no distinct
`qty_step`; the current `Decimal::ONE` value is an explicit broker-neutral
futures policy and must not be described as broker-observed data.

## Exit

`MATCH` means no production correction is needed in this slice. A
`MATERIAL_DRIFT_BLOCKED` result would stop the slice without modifying
production code. Independent
acceptance of this package may open Stage 8A-1 only. It does not open 8A-2,
Stage 8B or any order transport.
