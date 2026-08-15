# Stage 8A-4 implementation R3 — pure reconciliation reducer

## Authority

This implementation is a narrow realization of the independently accepted
Stage 8A-4 Design R2 at
`cc58c10d22db312cd83640f1c1e7fd86861a4594`. The independent review SHA-256 is
`43315b4653482998f0d112adbdcfc857afde8d1b68de94b3663b929c1ebad99e`.

The implementation admits only canonical `broker_core::BrokerTruthSnapshot`
under opaque, source-specific completeness evidence and reduces it to one
redacted diagnostic. It does not persist, publish, retry, resend or dispatch.

## Linear inputs and output

`Stage8a4DurableRequestContext`, `Stage8a4ReconciliationPolicy`, source timing
and completeness evidence have no public constructors. Their fields are
private and they are not `Clone`, `Debug`, `Serialize` or `Deserialize`.

`Stage8a4FreshTruthAdmission` owns the admitted truth and privately retains the
exact durable-binding, policy-binding and source-evidence-binding hashes under
which it was minted. The reducer rejects cross-paired context or policy before
candidate selection. Source evidence carries the exact canonical truth
multiset hash, and raw trade response count must equal the admitted raw trade
vector length.

The reducer consumes the
durable context, admission and policy and returns only
`Stage8a4ReconciliationDiagnostic`. That diagnostic contains redacted hashes,
bounded counts and semantic state. It contains no raw account, instrument,
order, trade, request or client-order identifier.

## Source-specific admission

- Orders use a typed complete non-paginated account snapshot proof.
- Positions use a typed complete account snapshot proof.
- Instruments use either exact-target resolution or an exhausted full-registry
  cursor proof.
- Trades use a policy-bound, gap-free union of inclusive-start/exclusive-end
  intervals. Saturated intervals are never complete. Their next midpoint split
  is deterministic and bounded by sealed depth and interval-count limits.
- Exact GET-order observation is a typed order plus HTTP request-start and
  response-received timing proof. It is optional tier-2 evidence. It cannot replace
  the account-wide orders snapshot and absence cannot prove no match.
- Every source is post-effect, fresh, account-scoped and cross-source-skew
  checked before admission.

## Deterministic reduction

Candidate precedence is exact client identity, then durable broker order ID,
then the fully bound Stage-8 shape and trusted event window. Tier 3 binds exact
account, venue instrument identity, side, quantity, order type, DAY TIF and
exact LIMIT price or absent MARKET price. Multiple candidates or exact-source
disagreement are `Conflict`; missing required shape and no candidate are
`StillUnknown`.

`BrokerTradeId` is the primary trade identity. Equal duplicates count once;
conflicting material duplicates are `Conflict`. Deduplicated matching trade
quantity must equal selected order `filled_qty`. The summary hashes a canonical
material view ordered by trade ID and excludes non-material `received_ts`.

At every tier, a present client or broker identity contradictory to the durable
request is `Conflict`. A supporting trade is compared with both the selected
order and durable request identities. If it matches one exact identity but
contradicts another present selected or durable identity, it is `Conflict`; a
trade with no matching selected or durable identity is unrelated and ignored.

Exact state keeps lifecycle and fill effect orthogonal. A cancelled or expired
order may therefore retain a partial fill. Unknown status remains
`StillUnknown`. Shuffled source rows and duplicate ordering produce the same
serialized diagnostic and semantic binding.

Post-admission `Conflict` and `StillUnknown` semantic bindings include the
current durable request, policy, admission and source-evidence bindings.
Pre-admission failures use a separate attempt binding containing the durable
request and request ID, policy, canonical truth attempt, source-evidence
attempt, outcome and reason. Canonical broker-truth row multisets are
order-independent, so replay/shuffle stability is preserved.

## Closed surfaces

The following remain closed: durable apply/journal, ACK/readiness publication,
Redis live consumption, broker dispatch, FINAM POST/DELETE, retry/resend,
runtime-live, real orders, Stage 8A-5 and Stage 8B. Historical cancellation
reconcilers are not imported as Stage-8 authority.

Acceptance of R3 may open only separately reviewed durable-composition
planning. It does not authorize an execution surface.
