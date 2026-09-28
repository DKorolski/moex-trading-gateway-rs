# Stage 8B-P1-e I1 supervisor foundation review boundary

Status: implementation review boundary with the I1 foundation R1 review
corrections; **not** I1 acceptance and **not** an operational activation
package.

Accepted predecessors:

- I0 source seam: `afda87a98ae3b0d0f4506292a162310f4b9068c0`;
- R10 design/checker: `d34e000c39f439ae981f9573c8b203a3dc8e3e85`;
- P1-d4 governance closure: `c2a9e1246dfdd59f3a6297268de907dedcb19903`.

## Implemented foundation

The source now contains the non-operational foundation required by I1:

- strict duplicate-key-rejecting supervisor JSON parsing and validation;
- exact embedded IMOEXF Hybrid high180 paper runtime construction;
- fixed loopback Redis DB15 URLs and a fresh generation/boot-bound consumer;
- verify-only deployment-manifest, key-type and group validation;
- bounded stale zero-PENDING consumer cleanup whose final exact-consumer
  pending/idle recheck and `DELCONSUMER` run in one pinned Redis-atomic script;
- fixed `NOMKSTREAM MAXLEN = 4096` health/readiness publication;
- exhaustive classification of all 22 durable restart outcomes;
- cause-preserving shutdown/exit, readiness and redacted telemetry contracts;
- separate S06 pending-only acquisition and S08 one-entry fresh polling;
- opaque `Stage8bP1eClaimedM10DeliveryV2` ownership for Ready sources;
- retained full instrument/timeframe/open/close/source-kind identity alongside
  the exact canonical bytes and hashes in that linear delivery;
- authenticated pre-callback routing between ordinary Ready and a surviving
  P1-d3 working LIMIT;
- route-specific already-acquired continuations with no second `XPENDING`,
  `XAUTOCLAIM` or `XREADGROUP` operation.

The working-LIMIT classifier reads only the authenticated replacement book.
It does not inspect M10 payload or callback output. This matters after an
untouched M10: the restart phase may be `SemanticCallbackCommitted` while the
book still contains the active working LIMIT.

No binary, service unit, provisioning action or VPS/DB0 activation is added
at this boundary.

## I1 foundation R1 review corrections

The two local P1 findings against the original foundation are closed in the
source candidate as follows:

1. `Stage8bP1eCoordinatorV1` owns the accepted first-wins shutdown latch.
   Error exit classes 70/71/73, the first request sequence and grace deadline
   survive an authenticated boundary. An ordinary boundary or stray grace
   event without an initiating shutdown keeps the owner loop running. Grace
   expiry after an initiating request returns 72 while retaining its cause for
   diagnostics.
2. Consumer discovery remains bounded to inventory 64 and deterministic
   prefix 16, but is no longer deletion authority. The pinned Lua contract
   re-reads the exact consumer and performs the zero-pending/idle-threshold
   check together with `DELCONSUMER` in one Redis atomic operation. Missing,
   newly active, insufficiently idle or nonzero-pending consumers are retained.
   A real-Redis test assigns an exact PEL delivery between discovery and the
   atomic operation and proves that both the consumer and delivery survive.

The cleanup amendment is additive. Runtime policy v1 remains historical;
deployable foundation config now pins
`stage8b-p1e-redis-runtime-policy-v2.json` and the external Lua artifact by
exact SHA-256. No cleanup fallback may delete a consumer when the server-side
atomic guarantee is unavailable.

## Fail-closed source gap

A complete deployable owner loop cannot yet be implemented from the accepted
default-feature production APIs without manufacturing schedule authority.

The continuations require these linear values:

- `Stage8bP1d1ExecutionScheduleAuthority` for generated Market execution;
- `Stage8bP1d3ScheduleStepAuthority` for working LIMIT/CANCEL evaluation;
- `Stage8bP1d3DayExpiryAuthority` for the deterministic Day boundary.

The only production bridge to the first value is crate-private
`stage8b_p1d1_schedule_authority_from_stage5e`, whose
`Stage5eScheduleProjectionBridgeInput` can be produced only inside the private
Stage 5E schedule owner. The only direct constructors for the P1-d3 values are
test/artifact-fixture gated. The canonical M10 schema, the first-boot source
bundle and the accepted P1-e Redis deployment manifest carry no steady-state
schedule source. The source itself records that a later operational adapter
may carry this value, but no such adapter or authenticated transport exists.

Consequently, none of the following is acceptable:

- deriving a schedule authority from an M10 timestamp alone;
- using `stage8b_p1d1_test_schedule_authority`,
  `stage8b_p1d3_test_step_authority` or
  `stage8b_p1d3_test_expiry_authority` in a release path;
- enabling an artifact-fixture feature in the deployable binary;
- silently treating every M10 as session-eligible;
- skipping the timer or working-LIMIT route.

## Narrow design correction required before the I1 owner loop

The next review should freeze one production schedule-source composition:

1. source artifact or pre-provisioned local transport and its owner;
2. authentication, freshness, operational identity and trading-day binding;
3. default-feature source-to-Stage5E projection facade;
4. one-use issuance rules for all three schedule authorities;
5. exact M10 predecessor/candidate and Day-boundary binding;
6. restart reissue and `SOURCE_FIRST_TIMER_DEFERRED` behavior;
7. failure/expiry handling and readiness degradation;
8. explicit confirmation that no FINAM POST/DELETE, broker dispatch,
   runtime-live or real-order surface is opened.

The most local option is a pre-provisioned authenticated broker-neutral
schedule stream beside canonical M10. It requires an explicit additive Redis
manifest/provisioning amendment. Changing canonical M10 or accepting a static
calendar file would instead require its own schema/freshness amendment. The
choice must be reviewed rather than inferred in implementation.

## Remaining I1 work after that correction

- production first-boot source facade F00..F17;
- executable owner loop over every accepted restart route;
- real source-first/timer checkpoint handling;
- signal task and task/panic supervision;
- process exit 0/64/66/67/70/71/72/73 evidence;
- readiness/health process tests and restart matrix;
- source checker, negative harness and immutable review package.

Until the schedule-source correction is accepted, this foundation remains
compile/test-only source work. Redis DB0/VPS, FINAM transport, broker dispatch,
runtime-live and real orders remain closed.
