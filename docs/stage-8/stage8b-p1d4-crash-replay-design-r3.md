# Stage 8B-P1-d4 crash/replay design R3 correction

Status: R3 design-only review candidate. P1-d4 source implementation remains
unauthorized until independent R3 acceptance.

Accepted predecessor:
`7dc7c802feca6e79d3a1a9902c181ad7b6afc506` (P1-d3 governance closure,
CLOSED / ACCEPTED).

Reviewed design history:

- R0 `b06c78b46d2a5b7a8209d58f1c327d7cf30ae98f` — HOLD;
- R1 `3a3f14f595b9672b23d421e7a857117fb2c578d2` — HOLD;
- R2 `16fe6dc535fdd744be9b814ab517386d83f52eac` — HOLD with P0=0,
  P1=2 and P2=0.

R3 is a narrow correction to R2. It exact-freezes the P1-d3 command/package
domain before dispatch-only candidate construction and replaces the impossible
global one-outcome scalar with scenario-aware typed outcome deltas. All other
accepted R2 corrections remain unchanged. No Rust, Cargo, workflow, config or
authority file is changed by this design revision.

## Normative artifacts

The active general acceptance matrix is:

```text
stage8b-p1d4-crash-replay-acceptance-matrix.csv
rows: 88
sha256: e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de
```

The sole active proof-cell registry is:

```text
stage8b-p1d4-scenario-frontier-matrix-v3.csv
cells: 92
cell IDs: P1D4C-001..P1D4C-092
scenarios: S01..S11
frontiers: F00..F20
sha256: 8c3122af016e860e3fa54a6f4143c50b9258c2a5a4bd2d15f9847a118a8582cc
```

The cell count remains 92 because R2 already enumerated both S09/F02 and
S09/F20. R3 strengthens those two rows with exact typed two-stage outcome
expectations; it does not add synthetic proof cells. The v1 and v2 registries
remain immutable historical review inputs and are not alternate contracts.

## Exact dispatch-only classifier domain

The dispatch-only classifier must not infer P1-d3 membership from
`Stage6DurableActionKind::Place` or `Cancel`. Before inspecting or returning a
dispatch-only candidate, a pure fail-closed domain predicate validates the
accepted command, durable snapshot, replacement package and P1-d3 working
book. A false predicate returns `None`; it never returns a partially validated
candidate or a blocked P1-d3 owner.

### PLACE predicate

Every condition is mandatory:

```text
accepted payload                    exact BrokerCommand::PlaceOrder
durable identity action             Stage6DurableActionKind::Place
command order_type                  OrderType::Limit
command time_in_force               TimeInForce::Day
command ttl_ms                      None
command qty                         > 0 and mathematically integral
command limit_price                 Some(value), value > 0
durable snapshot place shape        present and byte/field-equivalent
snapshot order_type                 OrderType::Limit
snapshot time_in_force              TimeInForce::Day
snapshot side/qty/limit             exact command values
request/account/instrument/DCID     exact durable identity values
comment/attribution                 exact P1-d3 attribution binding
accepted canonical payload hash     exact snapshot/command hash
semantic source binding             exact M10 Redis/semantic/payload/time tuple
replacement package                 authenticated and bound to committed seal
P1-d3 working book                  validates exact operational identity,
                                    generation, account, instrument,
                                    attribution, predecessor hash and sequence
```

`OrderType::Market`, non-Day LIMIT, LIMIT with `ttl_ms != None`, missing or
nonpositive limit price, zero/negative/fractional quantity, absent place shape,
shape mismatch, or missing/invalid authenticated P1-d3 replacement/book state
returns `None` before `Stage6Stage8bP1d3DispatchOnlyCandidate` or
`Stage8bP1d3DispatchPendingOwner` can exist.

### CANCEL predicate

Every condition is mandatory:

```text
accepted payload                    exact BrokerCommand::CancelOrder
durable identity action             Stage6DurableActionKind::Cancel
command ttl_ms                      None
request/account/instrument          exact durable identity values
attribution role                    HybridRuntimeOrderRole::Cancel
target BOID                         present in identity and command
authenticated P1-d3 registry        exactly one row for that BOID
canonical target TCID               resolved from that row
optional supplied target TCID       absent or exactly canonical
cancel DCID                         canonical request DCID and distinct from TCID
target row                          same account/instrument/attribution lineage
accepted canonical payload hash     exact snapshot/command hash
semantic source binding             exact M10 Redis/semantic/payload/time tuple
replacement/package/book            authenticated and cross-validated with
                                    operational identity, generation,
                                    predecessor and sequence frontier
```

Missing or ambiguous BOID resolution, supplied TCID mismatch, cancel-DCID to
target-TCID collision, changed target lifecycle row, TTL, semantic source,
package, book or identity returns `None` before a candidate/owner exists.

### Classifier order and predecessor non-interception

Restart classification order is exact:

1. existing accepted P1-d3 V3 journal-ahead classifier;
2. existing accepted P1-d2 journal-ahead classifier;
3. existing accepted generic P1 journal-ahead classifier;
4. new P1-d3 dispatch-only classifier, guarded by the complete predicates
   above;
5. unchanged fail-closed blocked fallback.

The new classifier may run only after the three accepted predecessor
classifiers return `None`. Even at step 4 it returns a candidate only for an
exact one-row `DispatchAttemptRecorded` suffix plus an exact P1-d3 domain
match. This ordering and the internal predicate are both mandatory; neither
may be omitted as supposedly redundant.

The source implementation must contain a production-reachable regression for:

```text
P1-d2 Market RequestAccepted
-> exact P1-d1 eligibility
-> DispatchAttemptRecorded persisted+reread
-> SIGKILL before provider.execute()/outcome append
-> restart
```

It proves:

```text
classify_stage8b_p1d3_dispatch_only_candidate == None
P1d3DispatchPending owners minted                 0
P1-d3 Stage5E authorities issued                  0
P1-d3 V3 outcomes appended                        0
second DispatchAttemptRecorded                    0
accepted P1-d2/fail-closed disposition             byte/enum unchanged
```

Equivalent negative fixtures are required for Market, non-Day LIMIT, LIMIT
with TTL and a syntactically valid LIMIT command without an authenticated
P1-d3 book. Mutations that move dispatch-only classification before a
predecessor classifier or weaken any domain predicate fail the source gate.

## Scenario-aware dispatch recovery outcomes

The scalar `outcome_append_count` from R2 is removed. Dispatch-only recovery
uses exactly two typed contracts.

### Simple request scenarios

Scenarios S01, S02, S03, S08, S10 and S11 require:

```text
DispatchAttemptRecorded rows before/after    1 / 1
target LaterFilled V3 delta                  0
request outcome V3 delta                     1
total P1-d3 V3 delta                         1
RequestFinalized delta                       1
second dispatch delta                        0
```

The one request outcome is typed according to the scenario. A generic or
wrong-kind V3 cannot satisfy the count.

### S09 target-first CANCEL

Both P1D4C-061 (S09/F02) and P1D4C-085 (S09/F20) must complete the entire
target-first lifecycle:

```text
DispatchAttemptRecorded rows before/after    1 / 1
target LaterFilled V3 delta                  1
recovered CANCEL V3 delta                    1
total P1-d3 V3 delta                         2
RequestFinalized delta                       1
target S_terminal delta                      1
S_cancel_recovered delta                     1
second dispatch delta                        0
source XACK                                  exactly last
```

Required order is:

```text
existing DispatchAttemptRecorded
-> target LaterFilled V3
-> target truth
-> target S_terminal persisted+reread
-> P1d3CancelContinuationPending audit
-> recovered CANCEL V3
-> RequestFinalized
-> recovered ACK
-> S_cancel_recovered persisted+reread
-> exact source XACK last
```

The intermediate audit immediately after target `S_terminal` must have
`restart_disposition == P1d3CancelContinuationPending`, target V3 delta one,
recovered-cancel V3 delta zero and RequestFinalized delta zero. It cannot be
credited as final completion. The final audit must carry both typed V3 record
IDs and evidence hashes in order and satisfy the complete delta block above.

A mutation that changes S09 to one generic V3, permits final success at target
`S_terminal`, omits `P1d3CancelContinuationPending`, reverses the two outcomes
or adds another dispatch fails closed.

## Evidence schema correction

`dispatch_only_recovery_contract.outcome_append_count` is forbidden. It is
replaced with `scenario_outcome_contracts` containing exactly:

```text
simple_request:
  scenarios = [S01,S02,S03,S08,S10,S11]
  dispatch_before = 1
  dispatch_after = 1
  target_later_filled_v3_delta = 0
  request_outcome_v3_delta = 1
  total_v3_delta = 1
  request_finalized_delta = 1

s09_target_first:
  scenarios = [S09]
  dispatch_before = 1
  dispatch_after = 1
  target_later_filled_v3_delta = 1
  recovered_cancel_v3_delta = 1
  total_v3_delta = 2
  request_finalized_delta = 1
  required_intermediate_disposition = P1d3CancelContinuationPending
```

The evidence also contains `classifier_domain` with exact PLACE and CANCEL
predicates and the predecessor classifier order. The checker validates the
entire structured values and the matrix byte hashes, not prose tokens alone.

All existing `Stage8bP1d4CrashReplayEvidenceV1` cell-level fields and marker
normalization remain unchanged. S09/F02 and S09/F20 sequence/audit fields use
the typed two-stage expectation frozen in v3.

## R3 negative obligations

The R3 mutation harness includes all R2 cases and additional targeted cases
for:

- Market PLACE interception;
- non-Day LIMIT, LIMIT with TTL, zero/fractional quantity, absent/nonpositive
  price and missing authenticated P1-d3 book;
- ambiguous/missing CANCEL BOID, TCID mismatch and DCID collision;
- classifier ordering before P1-d2 or generic P1;
- removal of exact command/snapshot/package binding;
- restoration of scalar `outcome_append_count`;
- S09 total V3 delta changed from two to one;
- omission or wrong type/order of either S09 V3 outcome;
- omission of the intermediate `P1d3CancelContinuationPending` audit;
- completion after target `S_terminal` without recovered CANCEL continuation;
- any second dispatch append.

Every mutation must fail the design gate. The R3 evidence records the exact
negative inventory count.

## Scope after R3 acceptance

R3 acceptance may authorize only:

- exact P1-d3 domain predicates and dispatch-only classifier after predecessor
  classifiers;
- the linear `P1d3DispatchPending` owner and no-second-dispatch continuation;
- scenario-aware simple and S09 typed outcome evidence;
- isolated subprocess hooks, ephemeral loopback Redis, read-only audits and
  deterministic evidence generation.

It does not authorize:

```text
operational Redis DB0/VPS
paper supervisor / P1-e
FINAM POST/DELETE
broker dispatch
runtime-live
real orders
partial fills
fees/slippage
replace/protective/bracket/multi-leg orders
Generation-2 production authorization
```

P1-e remains unauthorized until P1-d4 source acceptance and a separate
governance-only current-tree authority rebind.
