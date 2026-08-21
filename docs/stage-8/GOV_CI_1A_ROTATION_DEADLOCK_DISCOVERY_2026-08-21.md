# GOV-CI-1A — protected-base rotation deadlock discovery

Status: discovery evidence only. This document is not a retirement candidate and
does not authorize a governance bypass, Stage 8B-D R2, Stage 8B-S, FINAM execution,
Redis live consumption, broker dispatch, runtime-live, or real orders.

## Reproduced contradiction

The independent review requested a generation-4 to generation-5 terminal rotation
that keeps the canonical `.github/workflows/ci.yml` unchanged. The protected-base
contract cannot accept such a candidate from current `main`:

- protected base: `0ce76a334f12bf7b13e682ca976c9a4cde6be137`;
- fixed external authority snapshot: `8ce0acd60c7cb5cc5d25a27f6553077240658b57`;
- current-base `ci.yml` SHA-256:
  `b6ac51b4a5e014205e984939f53f46fd3fc02fe70a81135979a9cd9655eb2a14`;
- fixed-authority `ci.yml` SHA-256:
  `6133fb3900a9f11323df444c38760f6b71fdece927bfe2fb2cb411b5172d02f3`.

The old contract requires candidate `ci.yml` to equal the fixed authority snapshot.
Therefore an unchanged current `ci.yml` is rejected. At the same time its rotation
scope allows only `.github/workflows/stage5f-base-authority.yml` under the workflow
namespace; changing canonical `.github/workflows/ci.yml` back to the old bytes is
also rejected as out of scope.

Thus both exhaustive cases fail:

1. candidate keeps current canonical CI → external-authority equality failure;
2. candidate changes canonical CI → rotation scope failure.

Changing the contract in the candidate cannot resolve this because
`pull_request_target` executes the contract from the protected base.

## Required governance decision

The requested in-band retirement has no satisfiable candidate under the currently
active base-side contract. One explicit, review-visible recovery authority is needed:

1. approve a history-preserving administrative merge of a terminal retirement commit
   whose scope is limited to the Stage 5 authority workflow, generation state,
   rotation evidence and retirement document;
2. record an operator/reviewer attestation explaining the proven deadlock;
3. do not change canonical `ci.yml` in that retirement commit;
4. after retirement is present on `main`, create and independently review GOV-CI-1B
   from that exact base;
5. keep Stage 8B-D R2 and Stage 8B-S closed until GOV-CI-1B acceptance.

No force push, history rewrite, execution opening, or hidden check bypass is proposed.
