# M4-0 design-only expansion plan

Date: 2026-07-04

## Context

M3j is operationally closed:

- M3j-16b actual LimitCancel one-shot completed and reconciled;
- M3j-20 working LimitCancel micro completed and reconciled;
- operator signoff received;
- active orders = `0`;
- positions = `0`;
- continuous runtime-live remains disabled;
- command-consumer-to-real-FINAM remains disabled;
- Stop/SLTP/bracket/replace/multi-leg remain blocked.

M4-0 is design-only. It does not perform broker calls and does not enable any
new live trading boundary.

## Design principle

Do not jump from one-shot LimitCancel into continuous live runtime.

M4 must expand in separately reviewed layers:

1. Position lifecycle and broker-truth economics.
2. Persistent live audit and EOD reconciliation.
3. Command-consumer-to-real-FINAM gating.
4. Strategy runtime live-attachment policy.
5. Stop/SLTP/bracket/replace/multi-leg research and design.

Each layer requires its own evidence and operator approval before the next one
can be considered.

## Proposed M4 stages

### M4-1 tiny position lifecycle design

Scope:

- one account;
- one symbol: initially `IMOEXF@RTSX`;
- quantity = `1`;
- entry -> broker position snapshot -> exit -> final flat reconciliation;
- no strategy runtime;
- no command consumer;
- no Stop/SLTP/bracket/replace/multi-leg.

Design questions:

- entry type: marketable limit vs market;
- exit type: marketable limit vs market;
- maximum position lifetime;
- abort policy if broker truth is stale or unclear;
- required broker reports while in position.

### M4-2 economics / fees / EOD reconciliation

Scope:

- orders snapshot;
- trades snapshot;
- positions snapshot;
- cash/portfolio snapshot;
- commission/fee extraction if FINAM exposes it;
- realized PnL/EOD journal mapping.

Goal:

- prove that a tiny position lifecycle can be reconciled economically, not only
  operationally.

### M4-3 persistent live order audit

Scope:

- durable attempt id;
- client order id;
- broker order id;
- place/cancel raw-shape hash;
- broker-truth snapshots before/during/after;
- EOD immutable manifest.

Goal:

- every live boundary call has durable evidence before considering automation.

### M4-4 command-consumer-to-real-FINAM gate design

Scope:

- command consumer remains disabled by default;
- real-FINAM route requires explicit feature flag and operator arm;
- max orders / max qty / max notional / allowed account / allowed symbol;
- no blind retry after ambiguous state;
- kill switch and disarm semantics;
- duplicate command protection.

This stage is design-only. Enabling the consumer requires a later reviewed
implementation stage.

### M4-5 strategy runtime live-attachment policy

Scope:

- runtime may emit candidates only after explicit live-attach gate;
- initial policy should prefer one strategy/symbol/account;
- no RI/RTS expansion until IMOEXF lifecycle is stable;
- freeze/intent semantics must be tested separately.

### M4-6 Stop/SLTP/bracket/replace/multi-leg research

Scope:

- documentation and broker contract research;
- local fixtures;
- status semantics;
- cancel/replace semantics;
- bracket ownership and partial-fill semantics.

This remains blocked until earlier M4 layers are reviewed.

## Explicit non-goals for M4-0

M4-0 does not:

- send live orders;
- open a position;
- enable continuous runtime-live;
- enable command-consumer-to-real-FINAM;
- enable Stop/SLTP/bracket/replace/multi-leg;
- enable portfolio-level live strategy execution.

## Recommended next actionable stage

After M4-0 review, the safest actionable stage is:

`M4-1a tiny position lifecycle preflight design / no-send runbook`

It should define the exact operator approval text, preflight evidence, entry
type, exit type, maximum holding time, broker-report requirements, and abort
rules before any new live position test.
