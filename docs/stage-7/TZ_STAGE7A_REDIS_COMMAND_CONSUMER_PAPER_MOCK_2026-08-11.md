# Stage 7A — Narrow Technical Specification
## Redis Runtime Command Consumer, Paper/Mock Only

**Project:** broker-neutral MOEX runtime / FINAM gateway migration  
**Baseline / accepted predecessor:** `10e357825a701193d964975bb5769bd0745d4986`  
**Accepted predecessor status:** Stage 6E-R1 accepted; Stage 6 closed  
**Target slice:** Stage 7A only  
**Date:** 2026-08-11  
**Normative intent:** implementation specification for the programmer and independent-review acceptance contract.

---

# 1. Stage objective

Stage 7A must prove the **transport-side runtime command lifecycle over real Redis consumer-group semantics** while every broker effect remains paper/mock and local.

The target chain is:

```text
broker-neutral command envelope in paper Redis stream
  -> XREADGROUP / XAUTOCLAIM
  -> one canonical Stage 7A command handler
  -> Stage 6 durable identity/lifecycle authority
  -> deterministic process-local paper/mock outcome
  -> runtime-compatible CommandAck publication
  -> XACK only after successful settlement
```

Stage 7A is **not** a real-order stage and is **not** the final production-durability stage.

Its purpose is to prove that Redis delivery/redelivery cannot become a second execution identity and that the consumer cannot bypass the Stage 6 authority accepted at `10e3578...`.

---

# 2. Entry conditions

The implementation MUST start from exactly:

```text
10e357825a701193d964975bb5769bd0745d4986
```

or from a branch whose merge-base and predecessor gate prove that exact accepted Stage 6 closure.

Before Stage 7A code is admitted:

- Stage 6E-R1 acceptance is recorded in `current-status.md`;
- Stage 6 is marked CLOSED at `10e3578...`;
- Stage 7A becomes the only active implementation slice;
- Stage 7B, Stage 8+, runtime-live and real FINAM execution remain CLOSED.

---

# 3. Scope

## 3.1 In scope

Stage 7A MAY add:

1. a broker-neutral Redis command-consumer bridge;
2. real Redis consumer-group handling:
   - `XGROUP CREATE ... MKSTREAM`;
   - `XREADGROUP ... STREAMS <stream> >`;
   - cursor-correct `XAUTOCLAIM`;
   - `XACK`;
3. command envelope decoding;
4. redacted DLQ publication for permanent poison inputs;
5. runtime-compatible ACK publication;
6. a paper/mock execution adapter that cannot access broker transport;
7. Stage 6-backed admission/replay integration;
8. process-local command-consumer health/readiness/supervision;
9. deterministic fault injection for command lifecycle crash windows;
10. real-Redis integration tests;
11. Stage 7A gate, negative harness, closed-surface checker, preseal and handoff tooling;
12. Stage 7A documentation and status updates.

## 3.2 Explicitly out of scope

Stage 7A MUST NOT implement or enable:

- FINAM `POST` order;
- FINAM `DELETE` cancel;
- any broker network dispatch from runtime commands;
- `reqwest` broker invocation;
- real FINAM command execution;
- runtime-live;
- live micro;
- native Stop / StopLimit / SLTP / bracket;
- replace / multi-leg;
- persistent production Stage 6 file-journal composition;
- filesystem writer lock for production execution;
- cross-process exactly-once claim;
- continuous broker-truth reconciliation;
- ALOR-vs-FINAM live parity;
- Strategy 2 / Strategy 3 porting;
- changes to trading formulas or accepted Stage 5 strategy semantics.

Those belong to later stages.

---

# 4. Architectural boundary

## 4.1 Required dependency direction

Preferred implementation is a new crate, for example:

```text
crates/runtime-command-bridge
```

with dependencies no broader than required for the bridge:

```text
broker-core
strategy-runtime-core
redis
chrono
serde / serde_json
sha2
thiserror
tokio
uuid (only if needed for process-instance transport identity)
```

It MUST NOT depend on:

```text
broker-finam
finam-gateway
reqwest
rusqlite / OrderPathStore
```

The reason is structural: Stage 7A must be incapable, by dependency graph, of calling a FINAM order endpoint.

If the programmer chooses a different placement, it must preserve the same dependency rule and prove it with a checker. Adding a `finam-gateway -> strategy-runtime-core` dependency is not accepted.

## 4.2 Stage 6 remains the sole execution authority

The following historical components may be read as behavioral or failure-mode oracles only:

- `M3eCommandConsumer*`;
- `M3eCommandLifecycleStore`;
- `M3hRuntimeDryCommandEmitter*`;
- `M3h*LifecycleStore`;
- legacy SQLite `OrderPathStore`.

They MUST NOT become authoritative for:

- whether a request may execute;
- whether it may be redelivered as a new effect;
- request/client-order identity;
- broker-order correlation;
- reconciliation disposition;
- terminality.

There must not be two concurrent execution state machines.

Transport-local state is allowed only for transport concerns such as:

- Redis entry ID;
- consumer name;
- delivery count;
- ACK/DLQ publication attempt;
- health/readiness timestamps.

It must never override Stage 6 lifecycle facts.

---

# 5. Stream contract

Stage 7A MUST use a **paper-only stream namespace** and MUST NOT attach the new consumer to an ambiguous legacy/live-capable command stream by default.

Recommended defaults:

```text
command stream:
  finam_imoexf_paper:runtime:commands

ack stream:
  finam_imoexf_paper:runtime:command-acks

dlq stream:
  finam_imoexf_paper:runtime:commands:dlq

health:
  finam_imoexf_paper:runtime:command-consumer:health

readiness:
  finam_imoexf_paper:runtime:command-consumer:readiness
```

All configurable stream names must be validated to remain under the accepted paper namespace.

A configuration that targets a non-paper namespace must fail before connecting/processing.

## 5.1 Command envelope

Do not create a new command DTO if the accepted broker-neutral DTO already expresses the required command.

Use the accepted `broker-core` contract:

```text
Envelope<BrokerCommand>
schema_version = broker_core::SCHEMA_VERSION (currently 2)
msg_type = MessageType::Command
```

Allowed Stage 7A command semantics:

```text
BrokerCommand::PlaceOrder(MARKET)
BrokerCommand::PlaceOrder(LIMIT)
BrokerCommand::CancelOrder
```

No stop/bracket/replace/multi-leg expansion.

## 5.2 ACK envelope

Use the accepted broker-neutral ACK:

```text
Envelope<CommandAck>
msg_type = MessageType::CommandAck
```

The ACK must retain exact:

- `StrategyRequestId`;
- durable `ClientOrderId` when applicable;
- paper broker-order ID only if the accepted paper outcome contains one;
- explicit status/reason;
- host-observed `received_ts`.

No ALOR CWS-specific fields and no FINAM transport-body fields may enter this contract.

---

# 6. Identity rules

These are non-negotiable.

```text
Redis entry ID != StrategyRequestId
Redis consumer name != StrategyRequestId
consumer group != strategy identity
delivery count != lifecycle sequence
```

Execution identity remains:

```text
StrategyRequestId
  -> durable ClientOrderId
  -> BrokerOrderId(String), only when an accepted outcome has one
```

## 6.1 Duplicate delivery

If the same command is delivered again under:

- the same Redis ID; or
- a different Redis ID but the same `StrategyRequestId`;

the consumer must enter Stage 6 replay/dedupe logic.

It must not:

- mint a new request ID;
- derive a new client order ID;
- create a second `RequestAccepted`;
- invoke the paper effect a second time when Stage 6 says the effect is already represented or dispatch is unsafe.

## 6.2 Conflicting duplicate

Same `StrategyRequestId` with different immutable identity is not a poison-message convenience case.

Examples:

- different account;
- different instrument;
- Place vs Cancel;
- different durable client order ID;
- different Cancel target;
- different attribution/strategy ownership.

Required result:

```text
fail closed
manual-intervention / conflict reason
no broker/paper redispatch
no benign DLQ+XACK that loses the conflict
```

---

# 7. Canonical processing path

`XREADGROUP` and `XAUTOCLAIM` MUST call the same canonical function after obtaining a Redis entry.

Conceptually:

```text
process_redis_command_entry(entry, host_now, stage6_authority, paper_adapter)
```

There must not be separate lifecycle logic for "new" and "claimed" deliveries.

The source path may differ only in transport metadata.

---

# 8. Redis group behavior

## 8.1 Group creation

Use:

```text
XGROUP CREATE <stream> <group> <start-id> MKSTREAM
```

`BUSYGROUP` is an idempotent attach condition, not a reason to delete/recreate/reset a group.

## 8.2 Start position

Start policy must be explicit.

Recommended:

- operator-like newly created paper group: `Tail`;
- controlled deterministic test/replay group: `Beginning`.

No silent default that can unexpectedly consume an old historical command backlog.

## 8.3 Consumer name

Automatic consumer name must be **process-instance unique**.

A useful form is:

```text
<service>-<host/pid>-<boot-id>
```

The name is transport ownership only.

Do not persist or hash it into Stage 6 execution identity.

---

# 9. XAUTOCLAIM — required correction over old patterns

Stage 7A MUST implement cursor-correct `XAUTOCLAIM`.

Forbidden pattern for a multi-page claim loop:

```text
XAUTOCLAIM ... 0-0 COUNT N
XAUTOCLAIM ... 0-0 COUNT N
XAUTOCLAIM ... 0-0 COUNT N
...
```

Required pattern:

```text
cursor = 0-0
reply = XAUTOCLAIM(... cursor ...)
cursor = reply.next_start_id
...
```

with an explicit bounded termination rule.

The implementation must prove:

- more eligible entries than one page;
- deleted/missing PEL members if Redis returns them;
- no infinite busy loop;
- no starvation caused by restarting every claim page at `0-0`.

`XAUTOCLAIM` is recovery of **delivery ownership**, not authorization to execute again.

---

# 10. Processing classification

Each delivery must end in one of these conceptual classes.

## 10.1 Class P — permanent poison input

Examples:

- invalid JSON;
- wrong envelope schema;
- wrong message type;
- structurally undecodable command.

Required:

```text
redacted DLQ publish
-> only if DLQ publish succeeds: XACK
-> no Stage 6 lifecycle mutation
```

DLQ must contain safe metadata such as:

- schema version;
- source;
- stream;
- Redis entry ID;
- consumer group;
- reason code;
- payload SHA-256;
- created host timestamp.

It MUST NOT export raw payload/token/body/comment.

## 10.2 Class R — deterministic local/policy rejection with no effect

Examples:

- expired TTL before effect admission;
- unsupported Stage 7A order shape;
- account/instrument outside explicitly permitted paper profile;
- second unresolved lifecycle under the temporary Stage 7 policy.

Preferred result:

```text
runtime-compatible Rejected/Expired CommandAck
-> ACK publication
-> XACK
```

Do not DLQ ordinary valid commands merely because policy rejects them.

## 10.3 Class D — accepted/replayed Stage 6 lifecycle

Normal command processing enters Stage 6 authority.

Before invoking the paper adapter, the accepted order must be:

```text
RequestAccepted
-> DispatchAttemptRecorded
-> only then process-local paper effect
```

The consumer never synthesizes an "effect happened" fact before Stage 6 pre-effect authority exists.

## 10.4 Class U — uncertainty / lifecycle conflict / authority failure

Examples:

- conflicting duplicate;
- Stage 6 replay says blind redispatch blocked;
- injected paper outcome is inconclusive/reconciliation-required;
- Stage 6 authority error;
- identity mismatch after partial lifecycle establishment.

Required:

```text
NO benign DLQ
NO blind redispatch
NO terminal-success XACK
entry remains pending or is placed in an explicit reviewed hold state
command-path readiness becomes degraded/not-ready
operator-readable reason emitted
```

This distinction is critical: "cannot safely determine effect state" is not the same as "bad JSON".

---

# 11. XACK policy

`XACK` is a **transport settlement**, not proof that a broker command executed.

The only Stage 7A XACK cases are:

### Case 1 — successful command settlement

```text
Stage 6 transition accepted/replayed safely
AND required paper/mock outcome is recorded
AND runtime-compatible ACK publish succeeded
THEN XACK
```

### Case 2 — permanent poison

```text
DLQ publication succeeded
THEN XACK
```

### Case 3 — deterministic no-effect policy rejection

```text
runtime-compatible rejection/expiry ACK published successfully
THEN XACK
```

Do NOT XACK when:

- Stage 6 durable/replay authority errors;
- ACK publication fails;
- DLQ publication fails;
- state is inconclusive;
- dispatch safety requires reconciliation;
- identity conflict/manual intervention is unresolved;
- Redis settlement itself fails.

---

# 12. ACK delivery semantics

The ACK stream must be treated as **at-least-once**.

A failure can occur here:

```text
ACK XADD succeeds
-> process fails before XACK
-> command is reclaimed
-> equivalent ACK may be published again
```

Stage 7A is not required to build a second persistent ACK-outbox authority.

Instead it must prove:

1. redelivery does not re-execute the semantic command when Stage 6 blocks it;
2. any repeated ACK is equivalent for the same request;
3. runtime-compatible ACK application deduplicates exact repeated ACKs.

Do not claim "ACK exactly once".

---

# 13. Temporary single-unresolved-lifecycle policy

Stage 6E-R1 solved request-scoped issuance, but the accepted Stage 5G reducer remains fail-closed for a multi-slot restart.

Therefore Stage 7A MUST NOT silently create multiple simultaneous unresolved execution lifecycles per strategy instance.

Until separately reviewed multi-slot reconciliation exists:

```text
max unresolved/current execution lifecycle
per strategy instance = 1
```

The exact strategy-instance key must be documented and derived from accepted operational identity, not Redis consumer metadata.

A second different command while one lifecycle remains unresolved must be:

- rejected/deferred fail-closed;
- observable;
- not sent to paper effect;
- not allowed to create a second current Stage 6 lifecycle.

Exact duplicate redelivery of the same request is still allowed into replay/dedupe.

---

# 14. Paper/mock outcome adapter

Stage 7A must use an interface that is **structurally incapable of broker network dispatch**.

Example conceptual trait:

```rust
trait Stage7aPaperOutcomeProvider {
    fn outcome_for(
        &mut self,
        admitted_request: /* opaque accepted Stage 6 receipt or narrow view */
    ) -> Result<Stage6dPaperOutcome, Stage7aPaperError>;
}
```

The exact Rust signature may differ, but:

- it must not expose `reqwest`;
- it must not expose `broker_finam` transport;
- it must not accept a URL;
- it must not contain a real broker credential;
- the Stage 6 pre-effect receipt/capability must precede invocation;
- deterministic fixtures must cover MARKET, LIMIT and CANCEL;
- an injected `Inconclusive`/failure path must be testable.

Paper-generated broker IDs/trade IDs must be clearly synthetic and must never be reusable as real FINAM identifiers.

---

# 15. Health, readiness and supervision

Stage 7A must fix the ALOR-class supervision weakness before it is allowed to grow.

At minimum track:

```text
consumer_task_alive
last_successful_redis_poll_at
last_successful_claim_scan_at
last_command_settlement_at
redis_read_available
ack_stream_available
dlq_stream_available
stage6_authority_available
pending_uncertain_count
manual_intervention_required
```

Command-path readiness is false when:

- consumer task is dead;
- Redis source cannot be read within threshold;
- ACK settlement is unavailable;
- Stage 6 authority fails;
- an unresolved execution uncertainty requires reconciliation/manual intervention.

A detached task that dies while readiness remains true is an acceptance failure.

Supervisor policy may restart a transport task, but restart must not be allowed to create a new execution identity.

---

# 16. Trusted time rules

Stage 7A must preserve the temporal-authority repair from Stage 6E-R1.

For any provider/admission path that constructs local broker-truth observations:

- local receive time is minted by the trusted host/provider;
- collection start is minted locally;
- section observation/completion is minted locally;
- Redis payload timestamps are data, not authority;
- FINAM source timestamps are data, not local receipt authority.

For this stage, also close the carry-forward interval ambiguity where possible:

```text
restore_completed_at
  < collection_started_at
  <= row.received_ts
  <= section_observed_at
  <= captured_at
  <= validation_observed_at
```

If cached post-restore rows before collection start are intentionally allowed, that exception must be explicit and separately tested; do not retain stronger documentation than implementation.

---

# 17. Cross-process durability boundary

Stage 7A MUST state clearly:

> Stage 7A does not yet prove production crash-safe cross-process exactly-once execution because the integrated Stage 6 runtime composition is not yet wired to the production file-backed journal and writer lock.

Stage 7A MUST prove same-authority redelivery/fault windows.

Stage 7B will own:

- production file-backed Stage 6 journal composition;
- fsync-authoritative lifecycle;
- single-writer exclusion;
- process restart recovery;
- cross-process `Redis PEL -> Stage 6 replay -> settlement` tests;
- final command-path durability readiness.

Do not move those claims into Stage 7A evidence merely to close the stage faster.

---

# 18. Real Redis integration requirement

Mocks alone are insufficient.

The Stage 7A CI/review package MUST run against a real Redis instance and prove:

```text
XGROUP
XREADGROUP
PEL creation
XAUTOCLAIM
cursor advancement
XACK
ACK XADD
DLQ XADD
```

A GitHub Actions Redis service container or an equivalent hermetic local Redis instance is acceptable.

The test must not require FINAM credentials or public broker network access.

---

# 19. Fault-injection matrix

At minimum, inject failure after each point:

```text
F1  after XREADGROUP / before decode
F2  after decode / before Stage 6 admission
F3  after RequestAccepted
F4  after DispatchAttemptRecorded
F5  during paper outcome provider
F6  after Stage 6 paper outcome record
F7  before ACK XADD
F8  after ACK XADD / before XACK
F9  during XACK
F10 during DLQ XADD
F11 after DLQ XADD / before XACK
F12 consumer task fatal exit
F13 Redis source outage
F14 ACK stream outage
F15 DLQ stream outage
```

For each failure, the test evidence must identify:

- whether Redis entry remains pending;
- whether Stage 6 may be re-entered;
- whether paper effect may be invoked again;
- whether ACK may be repeated;
- whether XACK is allowed;
- expected readiness state.

---

# 20. Preferred source-change boundary

Preferred changed paths:

```text
Cargo.toml
Cargo.lock
crates/runtime-command-bridge/**
docs/stage-7/**
docs/current-status.md
docs/roadmap.md
docs/reviewer-onboarding-and-roadmap.md
scripts/stage7a_*
.github/workflows/... only if required for Redis integration gate
```

A minimal additive `strategy-runtime-core` bridge is allowed only if existing public Stage 6 authority cannot be safely consumed otherwise.

Any such bridge must be:

- narrow;
- additive;
- checker-pinned;
- unable to bypass Stage 6 ordering;
- accompanied by compile-fail/negative witnesses.

Avoid changing accepted Stage 5 semantics.

`broker-finam` and real endpoint code should have **zero Stage 7A production diff**.

---

# 21. Required deliverables

Programmer handoff must include:

1. exact commit SHA;
2. branch name;
3. source archive;
4. archive SHA-256 sidecar;
5. source-tree manifest with per-file SHA-256;
6. Stage 7A closure document;
7. machine-readable closure descriptor;
8. Stage 7A static checker;
9. closed-surface checker;
10. negative mutation harness;
11. preseal checker;
12. full gate log;
13. toolchain evidence;
14. focused debug test log;
15. focused release test log;
16. real-Redis integration log;
17. fault-injection report;
18. dependency-surface report proving no broker transport dependency;
19. updated `current-status.md` / roadmap state;
20. explicit carry-forward list for Stage 7B.

---

# 22. Mandatory gate

A proposed `stage7a_gate.sh` should run, in order:

```text
cargo fmt --all -- --check

python3 scripts/stage7a_check.py
python3 scripts/stage7a_closed_surface_check.py
python3 scripts/stage7a_negative_harness.py

# accepted predecessor / inherited gates
detached accepted Stage 6E-R1 gate
inherited Stage 6E / 6D / 6C-R1 as pinned by predecessor authority

# focused
cargo test -p runtime-command-bridge stage7a_ -- --nocapture
cargo test -p runtime-command-bridge stage7a_ --release -- --nocapture

# real Redis integration
<start hermetic Redis>
cargo test -p runtime-command-bridge --test stage7a_redis_integration -- --nocapture

# regression
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

python3 scripts/stage7a_preseal_check.py
```

Exact command names may differ if the package name differs, but equivalent coverage is mandatory.

---

# 23. Negative harness requirements

The negative harness must make targeted mutations and prove the gate rejects them.

At minimum mutate:

1. remove paper stream namespace check;
2. attach command stream to legacy/non-paper namespace;
3. add `broker-finam` dependency;
4. add `reqwest` dependency;
5. add FINAM POST/DELETE token;
6. bypass Stage 6 and call paper adapter first;
7. derive execution identity from Redis entry ID;
8. start every XAUTOCLAIM page at `0-0`;
9. XACK before ACK publication;
10. XACK when ACK publication fails;
11. XACK when DLQ publication fails;
12. DLQ an execution-uncertain/conflict request;
13. permit a second unresolved lifecycle;
14. replace request-scoped dedupe with Redis-entry dedupe;
15. allow raw payload in DLQ;
16. allow consumer death without readiness downgrade;
17. use broker/Redis timestamp as trusted local receive time;
18. allow row receipt before declared collection start if strict interval policy is chosen;
19. consult M3e/M3h lifecycle store for execution decision;
20. consult legacy SQLite `OrderPathStore` for execution decision;
21. claim cross-process exactly-once in Stage 7A documentation;
22. remove inherited Stage 6E-R1 gate;
23. omit release focused tests;
24. omit real Redis integration;
25. omit source-tree manifest/preseal binding.

The expected mutation count must be pinned in the closure descriptor and checked exactly.

---

# 24. Acceptance matrix

The full machine-readable matrix is delivered separately as:

`STAGE7A_ACCEPTANCE_MATRIX_2026-08-11.csv`

All rows marked `Blocking=YES` are mandatory.

Summary groups:

| Group | Required property |
|---|---|
| Lineage | Exact Stage 6 closure predecessor |
| Surface | No FINAM/live/broker effect |
| Identity | Redis metadata never becomes execution identity |
| Consumer group | Real XREADGROUP / cursor-correct XAUTOCLAIM |
| Settlement | XACK only after ACK or DLQ settlement |
| DLQ | Poison only; redacted; never hides uncertainty |
| Stage 6 | Sole lifecycle authority, durable-before-effect ordering |
| Dedupe | Same request never becomes second execution |
| Conflict | Fail closed/manual intervention |
| CF-1 | Max one unresolved lifecycle for initial Stage 7 path |
| Supervision | Dead consumer => not ready |
| Time | Trusted host local observation authority |
| Redis | Real Redis integration, not mocks only |
| Faults | All critical crash windows exercised |
| Governance | Stage 8+ remains closed |
| Handoff | Resealed clean source + evidence |

---

# 25. Exit criteria

Stage 7A is ACCEPTABLE only if an independent reviewer can state all of the following:

1. Redis new delivery and stale-pending recovery converge on one canonical processing path.
2. Redis entry identity cannot create or alter execution identity.
3. Stage 6 remains the only execution lifecycle authority.
4. Paper effect cannot happen before Stage 6 pre-effect ordering is satisfied.
5. Duplicate/redelivered commands cannot create a second semantic execution under the same Stage 6 authority.
6. Conflicting duplicates fail closed.
7. Permanent poison messages are redacted and settled only after DLQ persistence.
8. Execution uncertainty is never disguised as DLQ success.
9. ACK failure leaves the Redis command pending.
10. ACK redelivery is safe under at-least-once semantics.
11. XAUTOCLAIM cursor handling is correct under multi-page PEL recovery.
12. Consumer death and Redis settlement failures make readiness false.
13. The temporary one-unresolved-lifecycle policy is enforced.
14. Trusted local timestamps remain host/provider-owned.
15. No real FINAM execution surface is reachable or newly linked.
16. Real Redis behavior is proven in tests.
17. Workspace/inherited gates remain green.
18. The handoff is cryptographically bound to the exact source tree.

---

# 26. What Stage 7A acceptance opens

Independent acceptance of Stage 7A opens only:

# Stage 7B — production durability composition and cross-process recovery

Stage 7B should then own:

```text
Stage 6 file-backed journal
+ fsync authority
+ single-writer lock
+ actual paper runtime command-source attachment
+ process restart / PEL recovery
+ command-consumer readiness as production service dependency
```

It still does not automatically open FINAM real execution.

The next promotion path remains:

```text
7A Redis consumer semantics
-> 7B production durable composition
-> Gate 7→8
-> 8A protected FINAM adapter + ambiguous-outcome reconciliation authority
-> 8B bounded real POST/DELETE
-> Stage 9 continuous reconciliation
-> Stage 10 runtime-live readiness
-> Stage 11 ALOR live-micro oracle vs FINAM shadow/paper parity
-> Stage 12 controlled FINAM live-micro
```

---

# 27. Final programmer instruction

Implement **only Stage 7A**.

Do not opportunistically start Stage 7B or Stage 8 in the same commit.

The review unit must remain narrow enough that an independent reviewer can prove:

> Redis delivery/recovery is now real, but execution remains paper/mock and Stage 6 still has exclusive authority over whether a command may semantically execute.

That is the Stage 7A definition of done.

