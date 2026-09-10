# Stage 8B-P1-e I0 source-seam implementation

Status: source review candidate. The accepted design/checker predecessor is
`d34e000c39f439ae981f9573c8b203a3dc8e3e85` (R10). The accepted production
predecessor remains P1-d4 closure
`c2a9e1246dfdd59f3a6297268de907dedcb19903`.

## Implemented boundary

Every existing Redis resume path is split into two linear phases:

```text
route-exact Redis acquisition
  -> Stage8bP1ePostAcquisitionOwnerV1
  -> decide_stage8b_p1e_post_acquisition_latch
       -> RetainForRestart(Stage8bP1eRetainedSourceReceiptV1)
       -> Continue(Stage8bP1eContinuationPermitV1)
  -> route-exact continuation
```

The owner and permit have private fields and a private route enum. They are
not `Clone`, `Copy`, serializable, deserializable, reconstructible or
splittable. A continuation consumes exactly one permit. A route mismatch
consumes the permit and returns
`P1eContinuationPermitRouteMismatch`; it cannot return the embedded durable
owner, Redis transport or M10 source.

There are 21 public acquisition entry points and 21 public continuation entry
points covering the accepted 15 reclaim-required and five terminal logical
routes, including material P1-d3 dispatch and P1-d4 journal-ahead variants.
The former owner-plus-transport continuation signatures are removed.

Acquisition functions perform the one accepted route-exact reclaim or
terminal observation and construct the opaque owner. They do not parse a bar,
invoke the strategy callback/provider/schedule, publish a command, commit the
next replacement seal, revalidate publication or XACK. Continuation functions
do not perform a second delivery acquisition.

## Latch and terminal semantics

`Stage8bP1eShutdownLatchV1` is monotonic: the first request fixes cause, exit
class, grace deadline and request sequence. Later requests cannot replace it.
The accepted 3 cause by 3 arrival-location cross-product is covered by the I0
unit test; owner failure remains its distinct exit-70 supervisor cause.

When the latch is set, the decision consumes the acquired owner and returns a
diagnostic-only receipt. No continuation permit or source payload escapes, and
the exact Redis source remains at its pre-decision PEL/frontier state. When the
latch is clear, the decision emits one route-bound permit. A signal after that
linearization point cannot cancel the permit; a future supervisor may drain it
only to the route's accepted authenticated boundary and then recheck the
latch.

The zero-intent, P1-d2 truth, P1-d3 truth and P1-d4 truth paths resolve/XACK
only after permit consumption. P1-d4 publication revalidation also remains
post-permit and precedes its terminal XACK.

## Evidence mapping

The I0 tests bind their inventory to the accepted route/outcome fixture JSON
and its R10 counter amendments:

- 30 material route cells;
- 46 authenticated outcome fixtures;
- exact seven-counter effect profile for every fixture;
- FX10 post-permit P1-d4 revalidation;
- reconstructed semantic and recovered-cancel replacement-seal counts;
- preset-latch retention and route-mismatch zero-effect behavior;
- monotonic cause-preserving shutdown intent;
- compile-fail rejection of Clone, Copy, serde and the old continuation API.

The pre-existing runtime-durable-service tests now enter resume paths through
acquire, clear-latch decision and consumed permit. The accepted P1-d4 105-cell
SIGKILL suite remains the executable end-to-end effect proof and must run
twice on the clean committed I0 tree through the pinned R10 gate. The retained
gate output binds its source ref/tree, full logs, exact-test results, crash
evidence and artifact digest.

## Deliberately closed

I0 adds no supervisor binary, Cargo dependency, workflow, configuration,
service unit or deployment artifact. Operational Redis DB0/VPS activation,
FINAM POST/DELETE, broker dispatch, runtime-live and real orders remain
closed. Supervisor implementation is a later independently authorized slice.
