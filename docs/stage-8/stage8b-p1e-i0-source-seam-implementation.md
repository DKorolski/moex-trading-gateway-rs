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
terminal observation and construct the opaque owner. They do not invoke
`Stage8bP1PendingM10Delivery::parse_exact`, a strategy
callback/provider/schedule, publish a command, commit the next replacement
seal, revalidate publication or XACK. `reclaim_exact_binding` retains the
Redis-id, semantic-id and payload-hash checks during acquisition but returns
the still opaque delivery. LR04, LR12 candidate-source and LR15 perform the
identity-aware canonical parse only after consuming their continuation
permit. Continuation functions do not perform a second delivery acquisition.

## Latch and terminal semantics

`Stage8bP1eShutdownLatchV1` is monotonic: the first request fixes cause, exit
class, grace deadline and request sequence. Later requests cannot replace it.
The accepted 3 cause by 3 arrival-location cross-product is covered against a
real temporary Redis source and LR02 acquisition/permit continuation. For the
acquisition-in-flight location, a test-only one-shot barrier is entered from
inside the polled production acquisition function. The test observes that the
future has entered and cannot yet complete, sets the latch, releases the
barrier, and drains that same future; an entry counter proves that no second
acquisition occurred. The other two locations are after acquisition before
decision and immediately after a clear-latch permit decision. The earlier
preset-latch case remains as a separate fourth control. Owner failure remains
its distinct exit-70 supervisor cause.

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

Inventory and executable evidence are deliberately separate:

- `p1e_i0_inventory_pins_30_route_cells_and_46_effect_profiles` checks that
  the accepted 30-cell/46-fixture specification and seven-counter profiles do
  not drift. It is not counted as execution of those outcomes.
- The inherited P1-d4 105-cell SIGKILL drivers now perform an additional
  authenticated duplicate restart. Every source-bearing restart is acquired,
  passed through a real preset latch and retained with an observed all-zero
  effect vector. A separate byte-identical restart then receives one permit;
  the shared test adapter raises a real signal immediately after permit and
  still drains the linear future to the existing route-exact boundary.
- The same drivers continue to measure actual durable package generations,
  callbacks, provider/schedule operations, command publications, PEL/XACK
  dispositions, final owners and exact sequence allocations. Every shared
  Redis continuation adapter additionally brackets the real post-permit call
  with the seven-field I0 effect audit and asserts its outcome-specific
  observed vector. Dynamic P1-d3 pre-ACK and semantic outcomes are matched by
  the returned authenticated variant, including recovered-cancel, zero-, one-
  and multi-intent boundaries. Thus the fixture inventory is cross-checked
  against authenticated execution rather than used as a substitute for it.
- `p1e_i0_generated_market_fx05_through_fx10_observe_real_effects` directly
  executes FX05..FX10 from accepted GM00/03/05/06/07/10 crash frontiers. It
  measures actual replacement-seal, publication and revalidation calls and
  confirms that each non-terminal boundary retains one source PEL entry.
- LR04/FX04, LR12 candidate/FX20 and LR15/FX25 targeted tests prove acquisition
  parse count zero, post-permit parse count one, immediate signal retention,
  actual commit/callback/XACK counters and returned boundary.
- FX41/FX42/FX45/FX46 execute pending versus already-acknowledged source and
  both required signal checkpoints through latch-driven control flow. The
  original timer is constructed before source acquisition from the
  authenticated P1-d3 owner identity and recovery-seal provenance, with a
  deterministic evidence commitment and an exact due time. Reclassification
  consumes the returned authenticated owner and computes Retained only for a
  due timer bound to that owner; a due timer bound to a foreign authenticated
  identity computes Stale. Expected outcomes are assertions, not classifier
  inputs. A separate clear-latch next-owner-loop control increments the real
  execution probe for Retained and leaves it zero for Stale, so both stop
  checkpoints prove that execution was actively blocked rather than absent by
  construction.
- The LR02 3x3 in-flight/post-boundary test proves monotonic first-cause
  retention, exact source PEL, zero publication, boundary behavior and
  grace-expiry exit 72. The same test retains the preset-latch control as an
  additional case. The mismatch test consumes the wrong permit without
  exposing or mutating its authority.
- Separate compile-fail doctests reject `Serialize` and `DeserializeOwned` for
  both opaque types, in addition to Clone, Copy and the removed continuation
  API.

The accepted P1-d4 105-cell SIGKILL suite remains the executable end-to-end
effect proof and must run twice on the clean committed I0 tree through the
pinned R10 gate. The retained gate output binds its source ref/tree, full
logs, exact-test results, crash evidence and artifact digest.

## I0 source-correction closure

This correction candidate preserves the fixes in `1dbd4ac` and closes its two
remaining executable-evidence gaps:

1. LR02 acquisition-in-flight is synchronized inside one polled and incomplete
   acquisition future, while the preset-latch control remains distinct;
2. timer classification is owner/due-state derived, both latch checkpoints
   drive control flow, and a separate clear-latch next-step execution probe
   supplies the positive control.

The review-confirmed P1-02 correction in `1dbd4ac`—moving `parse_exact` out of
acquisition and auditing it as a strictly post-permit operation for LR04,
LR12 candidate-source and LR15—remains unchanged.

No accepted R10 contract, P1-d4 durable format or operational surface is
weakened by the correction.

## Deliberately closed

I0 adds no supervisor binary, Cargo dependency, workflow, configuration,
service unit or deployment artifact. Operational Redis DB0/VPS activation,
FINAM POST/DELETE, broker dispatch, runtime-live and real orders remain
closed. Supervisor implementation is a later independently authorized slice.
