# Stage 8B-P1-e R7 latch-aware source-seam design correction

Status: design-only review candidate. This commit changes no production Rust,
Cargo, workflow, active unit, deployed configuration, Redis or FINAM code.

R7 is the direct child of held R6 commit
`c0d7ee4c10e4d060fd15ea31f4593adcb793642b`. The immutable accepted
predecessor remains P1-d4 closure
`c2a9e1246dfdd59f3a6297268de907dedcb19903`. The independent R6 review is
bound by SHA-256
`8eb5d682927105bf6505a85ffdc249a281716adfaac5fc9e179c2d58ed4ed74b`.

R7 retains the R6 fifteen/five acquisition partition, 51-cell operational
overlay and stale-receipt-temp conflict. It corrects only the unimplementable
post-acquisition shutdown boundary and the checker gap that concealed it.

## 1. Selected model

R7 selects review option A: a narrow typed latch-aware source seam.

```text
exact reclaim or exact terminal lookup
  -> opaque linear Stage8bP1ePostAcquisitionOwnerV1
  -> one non-cancellable supervisor latch decision
  -> Continue(Stage8bP1eContinuationPermitV1)
       or
     RetainForRestart(Stage8bP1eRetainedSourceReceiptV1)
```

The acquired owner contains the exact durable phase owner, Redis transport and
canonical M10 delivery. Its fields and internal route enum are private. It is
not Clone, Copy, serializable, deserializable, reconstructible or splittable.
It exposes no payload, transport, Redis backend, durable owner or route
extractor.

Each accepted current `resume_*_with_redis`/terminal resolver name becomes a
continuation entry point: it consumes a continuation permit and cannot perform
lookup, XREADGROUP, XPENDING or XAUTOCLAIM. Separate route-exact acquisition
entry points perform the sole lookup/reclaim and return the opaque acquired
owner. The old owner-plus-transport signatures are removed, so they cannot
remain a bypass.

## 2. Latch linearization

The signal task may only set a monotonic shutdown latch. It never owns or
cancels the acquisition future. The sole lifecycle task awaits acquisition to
completion without racing it in `select!`, then consumes the acquired owner in
exactly one latch decision.

If the latch is set at that decision, `RetainForRestart` consumes and destroys
the in-process authority without parse, successor lookup, callback, provider,
schedule, publication revalidation, XACK or `AlreadyAcknowledged` resolution.
A reclaimed entry remains once in the PEL under the current consumer. A
terminal exact-lookup entry remains in its pre-decision PEL/frontier state;
that state is intentionally not resolved before restart.

If the latch is clear at the decision, creation of the continuation permit is
the linearization point. A signal arriving after that point may not cancel or
drop the permit. The exact route may advance only to its already accepted next
authenticated covering boundary, then the latch is rechecked before another
acquisition or effect. This closes the otherwise impossible check/set race
without treating a monolithic Redis wrapper as an implicit atomic drain.

## 3. Zero-intent and terminal paths

The zero-intent path is explicitly split:

```text
exact_delivery_for_evidence
  -> acquired owner
  -> latch decision
  -> continuation permit
  -> acknowledge_exact
```

No direct lookup-to-XACK edge remains after the authorized seam slice. P1-d2,
P1-d4 and P1-d3 terminal paths use the same protocol. P1-d4 publication
revalidation is continuation work and therefore occurs only after the permit.
Terminal routes still perform zero XAUTOCLAIM and remain independent of
`claim_idle_ms`.

## 4. Reclaim-required paths

All fifteen logical reclaim-required routes, including the three P1-d3
dispatch operation variants, obtain their sole exact reclaim in a route-bound
acquisition function. Parse, successor lookup, callback, provider, schedule,
publication and durable continuation are forbidden in acquisition. The
continuation entry point consumes the exact permit and performs no second
source acquisition.

The route binding is carried inside the private acquired owner. Passing a
permit to a different continuation fails closed while consuming that permit;
it cannot expose or reconstruct the original source owner.

## 5. Bounded source authorization

Independent acceptance of R7 authorizes only an I0 latch-seam source slice in:

```text
crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs
crates/runtime-durable-service/src/stage8b_p1_semantic.rs
crates/runtime-durable-service/src/lib.rs
```

The slice may add the three opaque protocol types, route-exact acquisition
entry points, one latch-decision function, continuation-permit signatures,
tests and re-exports. It may not add a supervisor binary, Cargo dependency,
Redis command family, FINAM dependency, broker dispatch, operational DB0/VPS
activation or live execution. P1-e supervisor source remains a later slice
after independent acceptance of I0.

## 6. Executable acceptance

The I0 source review must deterministically pause after acquisition and before
the latch decision for every fifteen reclaim-required and five terminal
logical routes. A pre-set latch must produce RetainForRestart and zero
post-acquisition effects. A clear latch must produce exactly one route-bound
permit; a signal immediately after permit creation may only cause a bounded
drain to the next accepted covering boundary.

Compile-fail/API tests must reject Clone, Copy, serde, field construction,
owner splitting, route extraction, direct invocation of an old
owner-plus-transport wrapper and a second acquisition. Source-body checks must
reject lookup-to-XACK, reclaim-to-parse, reclaim-to-provider/schedule,
reclaim-to-callback and acquisition futures raced in `select!`.

R7 itself is design-only. P1-f, installation, DB0/VPS, non-loopback Redis,
FINAM POST/DELETE, broker dispatch, runtime-live, real orders, partial fills
and protective/bracket/multi-leg orders remain closed. The nonblocking
`0 < child_pid <= u32::MAX` hardening remains deferred.
