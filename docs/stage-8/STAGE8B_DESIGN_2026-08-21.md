# Stage 8B design R1 — bounded engineering micro

Status: docs/checker-only design candidate; independent acceptance pending.

## 1. Authority and non-authority

Stage 8A is independently closed at accepted aggregate commit
`bf58b47fdef8af774a4107455dfcc6204e594283`. Its final review SHA-256 is
`72fa3c350dd34aef2d98230dec5547ba25bd7bc752b5b74eedf046e8502b13fc`.
This design starts from administrative closure commit
`0ce76a334f12bf7b13e682ca976c9a4cde6be137`.

This slice changes documentation and checkers only. It does not add or enable
HTTP transport, FINAM POST/DELETE, Redis XADD/XACK or live consumption, ACK or
readiness publication, broker dispatch, runtime-live or real orders. Acceptance
of this design may open only a separate Stage 8B implementation specification.
It does not authorize implementation, preflight, operator arming or a real
request.

## 2. Mandatory phase order

Stage 8B is split into separately reviewed transitions:

1. `8B-D` — this design contract;
2. `8B-S` — exact implementation specification and API/type-state topology;
3. `8B-I` — controlled implementation behind a no-send transport seam plus
   deterministic crash/replay rehearsal;
4. `8B-P` — fresh GET-only preflight evidence and one exact run authorization;
5. `8B-X` — at most one actual command followed by broker-truth reconciliation
   and an explicit post-run closure/signoff.

No acceptance implicitly opens a later phase. `8B-X` requires a new explicit
operator authorization naming its exact immutable run contract.

## 3. Exact run-contract shape

The later independently reviewed run contract must select exactly one command.
Its `action` is a singleton value, either `PLACE` or `CANCEL`; the design does
not authorize both. There is no automatic follow-up command. A LimitCancel pair
is out of scope because it is two broker effects.

The immutable run contract binds:

- one `StrategyRequestId`, durable `ClientOrderId` and Stage 6/7 command;
- one operator arm and one process boot identity;
- one FINAM account by exact `BrokerAccountId` plus a public SHA-256 binding;
- canonical instrument `IMOEXF` and venue symbol `IMOEXF@RTSX`;
- one selected action, side, quantity, order type and TIF;
- exact LIMIT price and maximum notional for PLACE, or exact durable target
  `BrokerOrderId` for CANCEL;
- exact accepted build, config, policy, endpoint and request-body hashes;
- issue time, expiry and trusted-time source;
- kill-switch generation, ownership lease and micro-budget generation.

PLACE is limited to `LIMIT`, `DAY`, maximum one lot. MARKET is closed. CANCEL
must target one exact currently working order already correlated to the same
account/instrument/strategy durable lifecycle. Stop, SLTP, bracket, replace,
multi-leg, conditional and native protective commands are closed.

The raw live account identifier remains outside Git and review archives. The
run package carries its SHA-256 binding while the operator-only local manifest
supplies the exact value. A mismatch fails before attempt recording.

## 4. Operator arm and max-one authority

The arm is opaque, durable, request-keyed, expiring and one-use. It is created
only after an independently accepted `8B-P` run package. It binds every field
of the exact run contract and cannot be cloned, serialized, reconstructed after
restart or minted twice for the same durable request.

The persistent budget is consumed before possible send and covered by the
current Stage 7B seal. Process restart never restores send authority. A second
invocation, second arm, changed command, changed body, changed endpoint or
changed build is blocked. Reconciliation may reuse identity for observation;
it never grants a second send.

## 5. Fresh read-only preflight

Immediately before any future effect, trusted production issuers must reread:

- current authenticated Stage 7B recovery seal and exact dispatch-ready
  Stage 6 command;
- accepted Stage 8A1 root/config/policy and current control state;
- Stage 7B composite readiness and single-broker ownership lease;
- current FINAM account, positions, target/account active orders and trades;
- exact instrument specification, schedule/session eligibility and trusted
  clock;
- ambiguity, unresolved lifecycle and consumed-budget state.

Preflight is GET/read-only. Caller-built snapshots, cached readiness, stale
broker truth, missing schedule, unreadable kill switch, non-`RunAllowed` state,
ownership conflict, unknown/orphan activity, unresolved lifecycle or identity
drift block before attempt recording. Readiness cannot be inferred from a
historical ACK.

The `8B-P` package must freeze maximum ages and prove freshness immediately
before the boundary. No values may be silently refreshed by the transport
adapter itself.

## 6. Durable attempt-before-send ordering

The only legal order is:

```text
fresh preflight
  -> consume one-use arm and durable micro budget
  -> append DispatchAttemptRecorded for exact request/body/endpoint/build
  -> fsync journal
  -> write and authenticate covering Stage 7B seal
  -> reread kill switch, arm expiry and immutable run contract
  -> enter exact transport boundary at most once
```

The transport cannot run before the attempt and covering seal are durable.
Redis markers, stream ownership and publication are never execution authority.
Failure to append, fsync, seal or revalidate is definitely no send and closes
the arm; it does not permit an automatic second attempt.

## 7. Crash and ambiguity windows

The implementation specification must distinguish at least:

| Window | Required result |
| --- | --- |
| before durable attempt | no send; no execution authority |
| attempt committed, transport not entered | no send; arm consumed; manual/new reviewed request required |
| transport entered, no complete response | outcome unknown; no retry; reconcile |
| complete response received, outcome not durably committed | recover from broker truth; no resend |
| broker outcome durable, Redis/ACK publication absent | settlement/publication-only recovery; no broker send |
| process killed at any boundary | fresh process reconstructs observation authority only |

`DefinitelyNotSent` is allowed only when the transport boundary proves it was
never entered. Timeout, disconnect, malformed/truncated success, 2xx without a
broker order ID, 429/5xx after possible send and response loss are ambiguous.

## 8. Idempotency and reconciliation

The exact durable `ClientOrderId` is reused only to correlate fresh broker
truth. An ambiguous request is never automatically sent again with the same or
a new ID. Current broker truth cannot rewrite durable identity. Fresh
account/orders/positions/trades/GetOrder evidence is normalized
through the accepted Stage 8A4 reducer and must resolve to one exact match,
exact terminal state, conflict or still unknown.

Empty, missing, stale or account-wide row counts cannot prove no order or flat.
Multiple matches, inconsistent broker IDs, cross-symbol evidence and lifecycle
regression are conflict. Conflict or still unknown disarms Stage 8B and requires
manual intervention. Post-run closure requires exact target order truth,
instrument-scoped position truth, account safety guard, durable final outcome
and operator signoff.

## 9. Kill switch and ownership

The persistent kill switch must be fresh, readable and exactly `RunAllowed`:

1. before arm issuance;
2. before durable attempt;
3. after covering-seal commit;
4. immediately before transport;
5. before any observation/settlement continuation.

Activation after possible send does not imply no order; it forces
reconciliation. Only FINAM may hold execution ownership for the exact
account/instrument/strategy scope. ALOR may remain read-only shadow but cannot
share execution authority.

## 10. Exact network boundary

The future transport may accept only non-serializable approved request parts
from the Stage 8B capability path. The run contract binds the exact official
FINAM TLS host, method and rendered route:

- PLACE: `POST /v1/accounts/{account_id}/orders`;
- CANCEL: `DELETE /v1/accounts/{account_id}/orders/{order_id}`.

Redirects, proxies, alternate hosts, arbitrary URLs, generic request methods,
caller-provided headers/routes and transport retries are forbidden. Secrets and
raw authorization headers never enter logs, reports or handoff archives. The
transport implementation and endpoint hashes require separate acceptance.

## 11. Evidence required before execution

Before `8B-X`, independently accepted packages must provide:

- exact source, archive, build, config, endpoint and body hash binding;
- full Stage 7B and Stage 8A inherited gates;
- compile-fail opacity/linearity and no-alternate-constructor evidence;
- deterministic no-send positive rehearsal and complete fault matrix;
- crash/restart tests proving zero duplicate sends;
- fresh GET-only FINAM auth/account/instrument/orders/positions/trades evidence;
- one exact operator arm and one-command blast radius;
- external host/method/route allowlist evidence;
- kill-switch, ownership, schedule, quantity, price and notional negatives;
- no-secret raw-response capture policy with redaction proof;
- outcome-unknown reconciliation and manual intervention runbook;
- post-run target-order, position, account-safety and operator-signoff schema.

## 12. Closed surfaces

This design keeps closed: Stage 8B execution; FINAM POST/DELETE; Redis XADD/XACK
and live consumer; ACK/readiness publication; broker dispatch; retry, resend or
re-arm; runtime-live; real orders; autonomous strategy attachment; unattended
or repeated send; Stop/SLTP/bracket/replace/multi-leg.

Independent acceptance may authorize only Stage 8B implementation
specification work. A real request remains forbidden until the later exact
`8B-X` package is independently accepted and the operator explicitly arms it.
