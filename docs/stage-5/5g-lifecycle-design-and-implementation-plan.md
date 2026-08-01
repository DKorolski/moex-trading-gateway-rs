# Stage 5G — paper lifecycle design and implementation plan

Status: design review candidate.
Date: 2026-08-01.
Accepted semantic predecessor:
`fb8245e2f91cfc1678548a1228e8558d9adc2181`.
Administrative closure predecessor:
`cac83da38725aeadd6d029a3078157c2ab7fa004`.

## Decision

Stage 5G attaches deterministic paper/mock broker feedback to the accepted
Stage 5F Hybrid semantic output. It reuses the existing broker-neutral ACK,
order, trade, position, timer, Stage 5C type-state and Stage 5D restart
contracts. It must not create a second strategy callback route, a second
broker-truth model, or a transport-shaped mock.

The first Stage 5G handoff is design-only. No Rust, Cargo, workflow, Redis,
FINAM or runtime-live surface is changed before this plan is independently
accepted.

## Objective

Prove that an accepted Stage 5F ordered semantic intent vector can move through
the complete broker-neutral paper lifecycle and converge to one deterministic
runtime state across:

- mock ACK outcomes;
- active, partial and terminal order evidence;
- correlated trade and target-instrument position truth;
- controlled timer callbacks and bar/timer arbitration;
- process restart at every durable semantic boundary;
- MR target/stop completion for Stage 5F rows F12–F15.

The result is paper lifecycle parity evidence. It is not command dispatch and
does not authorize a broker send.

## Stable boundary

```text
accepted Stage 5F semantic callback
                |
                v
ordered semantic intents + Stage 5C escrow
                |
                v
Stage 5G deterministic mock evidence coordinator
        |          |          |          |
       ACK      order/trade  position    timer
        \          |          /          /
         \---------+---------/----------/
                   |
                   v
existing Stage 5C broker-neutral callbacks
                   |
                   v
existing Stage 5D canonical restart package
```

Stage 5G owns only event admission, ordering, correlation and deterministic
paper reconciliation around these accepted boundaries. Stage 5F remains the
semantic authority. Stage 5C remains the callback/type-state authority. Stage
5D remains the persistence/restart authority. Broker Core remains the canonical
broker-truth and identity authority.

## Existing contracts to reuse

| Concern | Existing authority | Stage 5G use |
|---|---|---|
| ACK DTO and status vocabulary | `broker-core/src/command.rs` | Exact mock ACK evidence; no new status enum. |
| Pending/ACK policy | `broker-core/src/runtime_state.rs` | Exact request/client/broker ID matching and pending disposition. |
| Order/trade/position truth | `broker-core/src/operational_snapshot.rs` | Canonical lifecycle and target/account scoping. |
| ACK and broker-event callbacks | `stage5c_paper_host.rs` | Sole feedback callback/type-state path. |
| Timer arbitration | Stage 5C-k/l/m/n | One explicit event per bounded transition; no autonomous loop. |
| Restart package and recovery | `stage5d_persistence.rs` | Fresh-process deterministic restore and replay. |
| Semantic intent source | accepted Stage 5F matrix | Frozen ordered intents and pre/post fingerprints. |

These files are pinned in
`stage5g-lifecycle-entry-inventory.json`. A required change to a frozen
authority needs a separate reopening review; it cannot be hidden inside a
Stage 5G implementation patch.

## Identity and truth rules

1. `StrategyRequestId` is the only key allowed to resolve a pending strategy
   request. Client or broker IDs alone never clear pending state.
2. `ClientOrderId` and `BrokerOrderId(String)` are preserved exactly. Numeric
   surrogates, truncation and lossy normalization are forbidden.
3. A successful place ACK without a broker order ID remains pending for broker
   ID reconciliation; it cannot fabricate an ID.
4. Timeout, unknown-pending and ambiguous error outcomes preserve uncertainty
   and block an independent retry until broker truth resolves it.
5. Target-instrument order/position evidence is lifecycle truth. Account-wide
   non-target active or unknown orders remain a safety guard and diagnostic;
   row counts alone never prove target flatness.
6. Broker truth is authoritative over persisted working-set hints after
   restart. Contradiction is a blocker, never an implicit repair.
7. Duplicate evidence is idempotent only when identity and payload are exactly
   equal. A conflicting duplicate is terminal/manual-intervention evidence.
8. All callbacks are preflighted atomically before the first state mutation.
9. Event time is monotonic across semantic bar, ACK, broker event, timer and
   durable checkpoint watermarks.
10. Callback-generated intents re-enter the same ACK and broker lifecycle.
    They cannot bypass escrow or be dispatched.

## Stage sequence

### Stage 5G-a — entry contract and authority inventory

This design-only package contains:

- the accepted Stage 5F closure binding;
- source/API ownership inventory;
- the 54-case lifecycle acceptance design;
- proposed paper/mock development governance;
- fail-closed entry checker and negative harness;
- a commit-bound review handoff.

No production source changes are allowed in 5G-a.

### Stage 5G-b — mock ACK attachment

Add one opaque paper lifecycle session that consumes an accepted Stage 5F/5C
settled intent batch and broker-neutral `CommandAck` records. It must delegate
pending-state decisions to the existing Broker Core policy and then enter the
existing Stage 5C-i facade.

Acceptance includes accepted/submitted/recovered/rejected/timeout/
unknown-pending/duplicate/expired/error outcomes, exact ID correlation, place
and cancel semantics, deterministic duplicate handling and no synthetic broker
ID.

### Stage 5G-c — order, trade and position convergence

Attach canonical mock `BrokerOrderSnapshot`, `BrokerTradeSnapshot` and
`BrokerPositionSnapshot` evidence to the ACK-resolved capability and delegate
through Stage 5C-j.

Acceptance includes working, partial, filled, canceled, rejected, expired and
unknown status; monotonic filled quantity; exact target instrument/account;
trade correlation; target position side/quantity; account-wide safety guards;
identical replay and conflicting duplicate rejection.

### Stage 5G-d — timer and continuation arbitration

Use Stage 5C-k/l/m/n only. One explicit test event advances one state. Timer
output with intents re-enters Stage 5G-b/c. The same checkpoint cannot advance
both a bar and a timer. Equal or reversed timestamps are blocked.

There is no thread, scheduler, sleep, Redis read, network read or autonomous
loop in this slice.

### Stage 5G-e — deterministic restart and reconciliation

Serialize only through the accepted Stage 5D canonical restart package, drop
the source runtime, restore into a fresh runtime and reconcile fresh mock broker
truth before continuation.

Crash points cover pre-ACK, post-ACK, working, partial, filled-before-position,
terminal-position-applied, timer checkpoint and generated-intent escrow.
Replay must be idempotent. Missing or contradictory broker truth blocks with a
retained/retryable capability where the callback has not executed.

This is lifecycle restart simulation, not the Stage 6 durable command-ID
chain. Stage 5G stores no transport attempt and sends no command.

### Stage 5G-f — paper protective completion

Close the lifecycle ownership intentionally deferred by Stage 5F rows F12–F15:

- MR long target completion;
- MR short target completion;
- MR long stop completion;
- MR short stop completion.

Only mock broker evidence is allowed. Completion requires exact owner, cycle,
instrument, order/stop ID, side, quantity, chronology and the required target
position truth. A triggered/filled protective event without the matching flat
position cannot complete the cycle. Non-execution terminal cancellation cannot
invent an exit.

This slice does not implement native FINAM stop, SLTP or bracket placement.

### Stage 5G-g — lifecycle matrix and restart fingerprint freeze

Run the complete 54-case matrix in debug, release and default parallel modes.
Freeze:

- ordered input event vectors;
- ACK and broker-truth correlation decisions;
- callback counts;
- pre/post strategy-state fingerprints;
- lifecycle checkpoint fingerprints;
- restart package fingerprints;
- final reconciliation disposition.

Three focused runs must produce identical semantic and lifecycle evidence.

### Stage 5G-h — aggregate acceptance and closure

Produce a complete immutable handoff, aggregate all inherited and Stage 5G
gates, and request independent acceptance. Stage 5H same-input ALOR
differential replay remains separate. Stage 6 is not opened automatically by
5G implementation; it requires the stable macro-roadmap transition review.

## Acceptance families

The machine-readable inventory defines 54 mandatory scenarios:

| Family | Cases | Owner |
|---|---:|---|
| ACK and pending-state correlation | 10 | 5G-b |
| Order/trade/position convergence | 16 | 5G-c |
| Timer and continuation arbitration | 8 | 5G-d |
| Restart and reconciliation | 12 | 5G-e |
| Protective target/stop completion | 8 | 5G-f |

No case may be removed to obtain acceptance. A source-impossible case must be
classified explicitly as a negative invariant rather than enabled by a new
production seam.

## Failure ownership

- Pre-callback validation failures preserve the input capability when safe and
  expose a typed blocker suitable for corrected evidence retry.
- Once a source callback has executed, any later validation or settlement
  failure is terminal for that consumed capability.
- Ambiguous broker outcomes require reconciliation/manual intervention and
  never generate an independent retry.
- Unknown order status, identity mismatch, event-time reversal, quantity
  regression and contradictory position truth fail closed before callback.

## Governance for the development branch

Stage 5G-a proposes the following narrow continuation of the accepted local
development policy:

- develop on `stage5g-lifecycle`; keep `main` untouched;
- direct pushes to the development branch are backup/review artifacts, not
  release authority;
- use exact commit-bound ZIP handoffs and independent review checkpoints;
- require review of 5G-a before Rust implementation;
- require another review after 5G-c, after 5G-f and for 5G-h closure;
- decide protected-main/release governance separately before merge, deployment
  or Stage 6 activation.

This proposed policy is recorded in
`docs/adr/adr-stage5g-paper-mock-development-governance.md`. Its acceptance is
part of the Stage 5G-a review.

## Required gates

The 5G-a entry package requires:

```text
python3 scripts/stage5g_entry_plan_check.py
python3 scripts/stage5g_entry_plan_negative_harness.py
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/stage5f_forbidden_no_rg_gate.sh
python3 scripts/stage5g_entry_handoff_safety_check.py <archive>
```

The historical Stage 5B scanner is not applied directly to the additive Stage
5F/5G head: its accepted portable authority is executed from the immutable
detached snapshot by `stage5f_forbidden_no_rg_gate.sh` (87/87). The Stage 5G-a
changed-path gate independently proves that this design package changes no
Rust, Cargo or workflow surface.

Implementation slices additionally require focused debug/release/default
parallel matrices, exact Stage 5F closure inheritance, Stage 5C/5D frozen
authority checks and the Stage 5G no-I/O scanner.

## Closed surfaces

The complete Stage 5G development stage remains paper/mock and forbids:

- real FINAM `POST` or `DELETE`;
- FINAM transport invocation;
- Redis command consumption or consumer groups;
- broker dispatch/send/execution;
- runtime-live or `LiveReady` authorization;
- unattended or autonomous execution;
- real orders;
- native stop/SLTP/bracket placement;
- Stage 6 durable transport-attempt chain;
- Stage 7 command consumer;
- Stage 8 real execution.

## 5G-a definition of done

Stage 5G-a is ready for independent review when:

1. the accepted Stage 5F source/archive/evidence hashes are pinned;
2. all reused authority files and ownership rules are inventoried;
3. all 54 scenario IDs are unique and assigned to one slice;
4. the entry checker and every negative mutation pass;
5. no Rust, Cargo, workflow or operational file changed since the closure
   commit;
6. the source tree is clean and the handoff is bound to its exact commit;
7. the archive passes path, duplicate, symlink, secret and provenance checks.

After 5G-a acceptance, the next implementation slice is Stage 5G-b only.
