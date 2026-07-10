# Stage 4F — dirty-start / explicit adoption / manual-intervention policy

Status: accepted.

Date: 2026-07-10.

## Goal

Stage 4F adds the broker-neutral policy boundary that sits after validated
broker-truth application evidence and before any runtime bootstrap notification
is treated as acceptable.

This stage is still evidence/paper/mock only. It does not attach a strategy
runtime, does not emit runtime-live, does not connect the real FINAM command
consumer, and does not add any order POST/DELETE path.

## What this stage adds

- `evaluate_stage4_dirty_start_policy(...)`.
- `Stage4DirtyStartPolicyDecision`.
- `Stage4DirtyStartPolicyStatus`.
- `Stage4DirtyStartPolicyBlocker`.
- `Stage4DirtyStartPolicyBlockerKind`.
- `Stage4PositionAdoptionPolicyEvidence`.
- `Stage4OrderAdoptionPolicyEvidence`.
- `STAGE4_DIRTY_START_POLICY_SCHEMA_VERSION`.

Stage 4E also now carries the full `Stage4AdoptionDisposition` inside
`Stage4RuntimeBootstrapApplicationDecision`, so operator-facing Stage 4F
evidence can see the exact adoption attempt/allow/apply fields that Stage 4C
validated.

## Policy contract

Stage 4F accepts the dirty-start policy only when:

- Stage 4E application status is `Applied`;
- an applied runtime bootstrap snapshot is present;
- Stage 4E application evidence exactly matches the canonical
  `evaluate_stage4_runtime_bootstrap_application(validated)` decision for the
  same validated broker-truth report;
- no manual intervention is required;
- position adoption, if required, is explicit;
- order adoption, if required, is explicit;
- adopted position quantity exactly matches broker truth;
- adopted order count exactly matches broker truth for adoptable target orders;
- live authorization is still absent.

Position adoption and order adoption are evaluated separately.

Position adoption is explicit only when:

- `position_adoption_applied=true`;
- `position_adoption_attempted=true`;
- `position_adoption_allowed=true`;
- `adopted_target_position_qty == broker_truth target_position_qty`.

Order adoption is explicit only when:

- `order_adoption_applied=true`;
- `order_adoption_attempted=true`;
- `order_adoption_allowed=true`;
- `adopted_target_order_count == target_active_order_count - runtime_owned_target_order_count`.

Adoption is never inferred from broker truth. A non-flat target position without
explicit position adoption remains blocked. Adoptable target active orders
without explicit order adoption remain blocked.

Runtime-owned target active orders are not adoptable dirty-start orders. If a
target active broker order is already present in the restored runtime working
order set, Stage 4F treats it as runtime-owned lifecycle truth and does not
label the dirty-start disposition as `TargetActiveOrderRequiresAdoptionOrRepair`.

## Account-wide diagnostics

Non-target account-wide dirty state remains diagnostic by default:

- `account_wide_non_target_active_order_count`;
- `account_wide_non_target_open_position_count`;
- `account_wide_non_target_dirty_is_diagnostic=true`.

These fields do not block target runtime bootstrap policy unless a later
account-wide safety policy explicitly promotes them.

## Fixture-backed cases

Stage 4F tests cover:

- clean flat policy accepted without adoption;
- full adoption evidence carried from Stage 4C through Stage 4E into Stage 4F;
- position adoption requires attempted/allowed/applied and matching quantity;
- order adoption requires attempted/allowed/applied and matching count;
- application evidence from a different validated report is blocked;
- applied application evidence with blockers is blocked;
- non-ready validated report plus tampered `Applied` application evidence is
  blocked;
- runtime-owned active target order does not require order adoption;
- manual-intervention state blocks runtime notification policy;
- account-wide non-target active order remains diagnostic by default.

## Safety boundary

Stage 4F does not authorize:

- runtime-live;
- `command-consumer-to-real-FINAM`;
- strategy-driven real FINAM orders;
- real POST/DELETE order endpoints;
- Stop/SLTP/bracket/replace/multi-leg.

`Stage4DirtyStartPolicyDecision.no_live_authorization` remains `true`.

## Follow-up

Stage 4G continues with runtime lifecycle ordering evidence. Any move toward
runtime-live or real command consumption remains out of scope without later
accepted gates.
