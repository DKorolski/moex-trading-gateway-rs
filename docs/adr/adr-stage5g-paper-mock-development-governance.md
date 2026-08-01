# ADR: Stage 5G paper/mock development governance

Status: proposed for independent Stage 5G-a review
Date: 2026-08-01
Accepted Stage 5F source: `fb8245e2f91cfc1678548a1228e8558d9adc2181`
Stage 5F closure commit: `cac83da38725aeadd6d029a3078157c2ab7fa004`

## Context

The accepted Stage 5F development ADR deferred a repository-governance choice
before Stage 5G activation. Independent Stage 5F-e review accepted the complete
immutable handoff and explicitly unlocked Stage 5G for paper lifecycle, mock
ACK/order events, deterministic restart and reconciliation design. Real FINAM
order endpoints, Redis live consumption, runtime-live and unattended execution
remain forbidden.

Stage 5G is still local no-I/O engineering. Requiring release/deployment
governance for each design or paper test commit would add operational ceremony
without reducing broker blast radius. At the same time, Stage 5F closure and
the future release boundary must remain independently reconstructible.

## Proposed decision

1. Create `stage5g-lifecycle` from the immutable Stage 5F closure commit.
2. Keep `main` untouched throughout Stage 5G paper/mock development.
3. Permit direct pushes to `stage5g-lifecycle` only as development backup and
   exact review refs; they carry no merge, release or deployment authority.
4. Require commit-bound ZIP plus SHA-256 for independent review checkpoints.
5. Require independent acceptance of Stage 5G-a before lifecycle Rust code,
   after ACK/order/position convergence, after protective/restart convergence,
   and for aggregate closure.
6. Preserve Stage 5F, Stage 5C and Stage 5D authorities by exact source hashes
   and snapshot gates. Reopening one requires a separate review.
7. Make a separate governance decision before any merge to `main`, deployment,
   Stage 6 durable command-chain activation, Redis consumer, FINAM transport or
   runtime-live work.

## Consequences

- Development remains efficient and locally reproducible.
- `main` continues to represent the last independently governed integration
  point.
- Every accepted lifecycle milestone has an immutable source snapshot.
- Stage 5G cannot silently become live-capable because its branch policy and
  technical gates both keep all external execution surfaces closed.

## Rejected alternatives

- Treating a development-branch push as release authority.
- Making hosted CI administration the functional focus of Stage 5G.
- Merging design-only work to `main` before Stage 5G acceptance.
- Using absence of network credentials as the only no-send control.

This ADR becomes accepted only if the Stage 5G-a review accepts it. Until then,
only design, inventory, checker and handoff work is authorized on the branch.
