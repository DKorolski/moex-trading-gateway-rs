# Stage 8A-4 durable composition I4 design R2

## Authority and scope

I3 R6 is independently accepted and closed at
`593ff255ef7826a22e66c9aff6f7ea47acf47644`. Its acceptance review SHA-256 is
`1da167c3e7f1266473133d2d8a1412906a26d7f83b5dc026ce84dc7969090257`.
I4 Design R1 `06bb09fa13431d0ae34039f37497d4f37914f022` was not accepted; this
docs/checker-only R2 closes its three P1 ambiguities. Production Rust, Cargo and
workflows remain byte-identical to I3 R6.

I4 is a **read-only / no-effect composition**. It may reread and authenticate
the on-disk seal, refresh mixed replay without appending, read current
control/config sources and sample current broker truth/readiness. It may not
append or publish anything, repair a seal, mint execution authority, dispatch
or send.

I4 composes two independent authorities:

1. durable terminal authority reconstructed under the Stage 7B
   `Stage7bRecoveryReadyOwner` from a complete Stage8A4 V2 transition, its exact
   V1 suffix, final F1 and an authenticated current S1 already covering the
   accepted frontier;
2. ephemeral current readiness evidence issued from current trusted sources
   after durable terminal reconstruction.

Historical settlement cannot imply current readiness. Current readiness cannot
create, change or suppress the canonical durable ACK facts or identity.

## Type-state boundary

The controlled implementation must introduce separate non-`Clone`,
non-`Copy`, non-`Debug`, non-`Serialize`, non-`Deserialize` crate-private types:

- `Stage7bStage8a4TerminalAuthority` — broker-neutral durable authority;
- `Stage8a4I4TerminalAckFacts` — exact timestamp-free ACK facts;
- `Stage8a4I4CurrentReadinessEvidence` — current read-only readiness evidence;
- `Stage8a4I4DerivedAckReadinessFacade` — consumed composition result.

There is no public constructor, field access, raw journal input, caller-built
seal, caller-built checkpoint or digest-only constructor. The facade exposes at
most a bounded redacted diagnostic. It is not accepted by Redis settlement,
FINAM transport, execution-capability minting or runtime-live attachment.

## Durable terminal derivation and seal policy

The Stage 7B owner must:

1. reread and authenticate the current on-disk S1;
2. perform a read-only refresh of the version-aware mixed replay;
3. require that the existing S1 already covers the accepted current F1 and
   full mixed frontier;
4. derive no authority if S1 lags or any binding differs.

I4 must never call, emulate or reuse a path that can invoke
`advance_recovery_seal(...)`. In particular, the existing Stage 7 paper ACK
helper `authorize_finalized_ack(...)` cannot be reused unchanged because it can
advance a lagging seal. A lagging, missing, malformed, unauthenticated or
unreadable S1 fails closed and remains unmodified.

Terminal authority additionally requires:

- exactly one reconciliation V2 for the durable request;
- deterministic suffix state `Complete`;
- required `RequestFinalized` record;
- V2 transition `Exact`, never either hold variant;
- exact operational identity, runtime fingerprint, accepted command payload
  and durable request identity;
- no Pending, partial, corrupt, conflicting or uncovered history.

An I3 receipt is neither sufficient nor required after restart. Authority is
reconstructed from durable history. The separate
`settlement_authority_fingerprint_sha256` may bind current checkpoint, seal
generation and S1, but those current authority facts never enter stable ACK
identity.

## Exact canonical ACK facts

`Stage8a4I4TerminalAckFacts` freezes exactly:

```text
strategy_request_id: StrategyRequestId
durable_client_order_id: ClientOrderId
broker_order_id: Option<BrokerOrderId>
status: CommandAckStatus
reason_code: Option<CommandAckReasonCode>
terminal_request_ack_identity_sha256: SHA-256
```

The sources are fixed:

- `strategy_request_id` is the durable Stage 6 request identity;
- `durable_client_order_id` is that request's durable `ClientOrderId`;
- for CANCEL this is the cancel request client ID, never the target order's
  client ID (unless the byte strings coincidentally match);
- `broker_order_id` is only `known_broker_order_id` established by durable
  mixed replay; current broker truth cannot fill it and no ID is fabricated.

Mixed replay order/trade evidence has one durable identity result. Therefore,
when selected V2 order ID is `None` but material V1 `BrokerTradeObserved`
establishes `B1`, the ACK broker ID is `Some(B1)`. The selected-order `None`
does not erase `B1`, and a different current snapshot cannot replace it.

### Timestamp model

I4 uses **Model A**: durable ACK facts are timestamp-free.
I4 does not construct a full `CommandAck`. `received_ts` belongs to a future publication
slice and is excluded from stable terminal ACK identity, restart semantic
equality and durable duplicate equality. `Utc::now()` is forbidden in I4 ACK
fact/identity derivation.

### Sole canonical identity

I4 reuses the existing Stage 7B
`terminal_request_ack_identity_sha256` **exactly**. It introduces no second
request-level ACK identity or hashing domain. Reuse is sound because the
identity already binds operational identity, durable request/client/broker ID,
canonical command SHA-256, final disposition, final record ID/sequence and ACK
schema. The canonical command binds PLACE versus CANCEL; exact V2/suffix and
final disposition deterministically imply the status/reason table below. A
trade-established `B1` is first promoted to the recovered request's
`known_broker_order_id`, so the existing identity binds it as well.

No current S1 generation, current checkpoint, readiness fingerprint,
`observed_at` or `valid_until` enters this identity. These may enter only the
separate settlement/readiness authority fingerprints.

## Canonical status/reason mapping

PLACE:

| Durable transition | Final disposition | Status | Reason |
|---|---|---|---|
| ExactWorking | Completed | Recovered | RecoveredByBrokerTruth |
| ExactTerminalFilled | Completed | Recovered | RecoveredByBrokerTruth |
| ExactTerminalCancelled | Completed | Recovered | RecoveredByBrokerTruth |
| ExactTerminalExpired | Completed | Recovered | RecoveredByBrokerTruth |
| ExactTerminalRejected | Rejected | Rejected | BrokerRejected |

CANCEL:

| Durable transition | Required cancel outcome | Status | Reason |
|---|---|---|---|
| ExactWorking | none | none | unresolved |
| ExactTerminalFilled | ExecutionObserved | Recovered | RecoveredByBrokerTruth |
| ExactTerminalRejected | AlreadyTerminalNonExecution | Recovered | RecoveredByBrokerTruth |
| ExactTerminalCancelled | Canceled | Recovered | RecoveredByBrokerTruth |
| ExactTerminalExpired | AlreadyTerminalNonExecution | Recovered | RecoveredByBrokerTruth |

Every emitted terminal fact has the reason shown as `Some(reason)`. CANCEL
ExactWorking emits no ACK facts. `ReconciliationConflictHold` and
`ReconciliationStillUnknownHold` derive no terminal ACK, publication,
settlement, retry, resend or readiness authority. The ACK describes command
finalization; it does not rewrite broker order lifecycle.

## Exact current-readiness issuer

The only issuer path is conceptually:

```text
current Stage7bRecoveryReadyOwner
+ accepted Stage8a1AuthorityRoot and Stage8a1AcceptedExecutionConfigV1
+ current Stage7bCompositeReadinessSnapshot
+ current BrokerTruthSnapshot
+ current BrokerReadinessSnapshot
+ current Stage8A1 control/schedule/policy sources
-> crate-private Stage8a4I4CurrentReadinessEvidence
```

Inputs are read by the trusted owner/issuer path, not supplied as public caller
snapshots. The path does not require or mint `Stage8a1OperatorArmAuthority`,
`Stage8ExecutionCapability`, an order builder, a send continuation or any
PLACE/CANCEL transport object.

The evidence exact-binds:

```text
operational_identity_sha256
runtime_config_fingerprint_sha256
BrokerAccountId / AccountId
InstrumentId
strategy_id and strategy_instance_id / Stage8 authority scope
accepted Stage8a1AuthorityRoot identity
accepted config/policy fingerprint
current source evidence fingerprint
observed_at
valid_until
```

The source fingerprint covers all current control, composite readiness, broker
truth/readiness and schedule inputs plus their revisions. Account/instrument,
strategy scope, operational identity and runtime config must equal the terminal
authority and accepted root/config bindings. Cross-account, cross-instrument,
cross-strategy or cross-runtime composition fails closed.

## Current readiness policy and lifetime

Generic `Ready` is deliberately conservative. It requires:

- fresh current control with `RunAllowed`;
- current Stage 7B composite readiness;
- fresh broker truth and broker readiness;
- open schedule and accepted policy/config;
- exact identity/scope bindings;
- zero account unknown/orphan ambiguity;
- `account_active_orders_count == 0`;
- `target_active_orders_count == 0`;
- all frozen Stage8A1 readiness rules.

Thus a historical PLACE `ExactWorking` ACK can be valid `Recovered` while its
still-active order forces generic readiness to `Blocked`. I4 does not define a
repair/CANCEL readiness class; a future typed `EntryBlockedRepairOnly` class
requires separate design acceptance and cannot be represented as generic
`Ready`.

The evidence is ephemeral:

```text
valid_until = min(all current trusted source expiries)
composition/consumption requires now < valid_until
```

Expiry or any source revision change invalidates readiness but leaves the
historical terminal ACK unchanged. No cached `Ready` survives restart; current
sources must be sampled again.

The following always block readiness while preserving a valid historical ACK:

- `StopRequested`;
- missing, unreadable, malformed, stale or expired current control;
- stale/degraded composite readiness;
- stale broker truth or broker readiness;
- closed/unknown schedule;
- account unknown/orphan ambiguity;
- any account-wide or target active order;
- identity, authority-root, config, policy, source-revision or scope mismatch.

No I3 post-effect control snapshot can be reused as current readiness evidence.
No blocked state is normalized to ready.

## Read-only / no-effect boundary

Allowed reads only:

- on-disk S1 reread and authentication;
- read-only mixed replay refresh;
- current control/config/policy/source reads;
- current broker-truth and broker-readiness sampling.

Forbidden effects:

- Stage 6 journal append;
- V2 or suffix append;
- `RequestFinalized` append or second finalization;
- recovery-seal advancement/write/repair;
- ACK/readiness publication;
- Redis `XADD`/`XACK` or command consumption;
- FINAM/network send and broker dispatch;
- retry, resend, re-arm or execution-capability minting.

## Restart and duplicate semantics

Normal completion, immediate duplicate derivation and fresh-process restart
produce equal timestamp-free ACK facts and exactly the same existing Stage7B
terminal identity. Duplicate/restart derivation performs no durable mutation or
broker effect. Current readiness is always freshly sampled and may legitimately
differ or be absent after restart without changing ACK equality.

Publication knowledge, when added by a later separately accepted slice, may
classify canonical/duplicate/conflict but cannot alter durable ACK facts.

## Required implementation order after R2 acceptance

1. Add broker-neutral completed-transition facts to mixed replay.
2. Add owner-mediated terminal authority with read-only exact S1 validation and
   no seal repair.
3. Add timestamp-free ACK facts reusing the Stage7B terminal identity.
4. Add private FINAM-gateway readiness issuer and composition.
5. Add PLACE/CANCEL/hold/Pending/restart/duplicate/expiry/active-order matrices
   and compile-fail privacy tests.
6. Add I4 implementation checker, negative harness and immutable handoff.

Each step remains read-only / no-effect. ACK/readiness publication or Redis
`XACK` requires a later explicitly accepted slice.

The design gate proves production Rust, Cargo and workflows equality directly
against accepted I3 R6. It does not alter the repository-wide legacy forbidden
scanner whose frozen baseline predates accepted Stage 6/7 workspace additions.

## Closed surfaces

- ACK/readiness publication and Redis `XADD`/`XACK`;
- Redis command consumption and Redis live;
- FINAM POST/DELETE and all network sends;
- broker dispatch and retry/resend/re-arm;
- execution-capability/operator-arm minting from I4;
- runtime-live and real strategy orders;
- Stage 8A-5 and Stage 8B.
