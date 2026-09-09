# Stage 8B-P1-e R9 boundary and regression closure

Status: narrow design/checker correction candidate. Production Rust, Cargo,
workflow, active unit, deployment, Redis and FINAM code are unchanged.

R9 is the direct child of held R8
`fcac93e47e6dbb2f5c96c0fa28ce1c99cd603b3e`. The immutable accepted
predecessor remains P1-d4 closure
`c2a9e1246dfdd59f3a6297268de907dedcb19903`. R9 retains Option A, the opaque
linear owner, single-use route-bound permit, 15 reclaim / 5 terminal logical
partition, terminal observations without repeated acquisition, the
cause-carrying shutdown intent and the exact three-file I0 production
allowlist.

## Reattachment checkpoint is not a durable seal

LR02, LR04, LR10 and LR13 only authenticate the existing durable phase and
bind Redis transport/source ownership. This is a safe reattachment checkpoint,
not a newly committed durable covering seal. If shutdown is latched after the
permit, the route stops there: the source remains pending and no publication,
S_truth or XACK may occur implicitly.

E05 is deliberately narrower. It applies only when a normal owner-loop already
held S_ack before the shutdown request. It never applies inside LR04, LR10 or
LR13. E25 has precedence for an in-flight permit and completes exactly the
selected V2 route boundary before its latch recheck. Thus one durable state,
source disposition and signal position has one normative stopping point.

## Source before timer, with two latch checkpoints

The due-timer route accepts both terminal source outcomes: exact pending uses
one XACK reply 1; an absent exact PEL member with a continuous group frontier
is AlreadyAcknowledged and performs zero XACK. Both first establish an
authenticated source-resolution checkpoint. A latch check follows. If set,
the original authenticated timer is retained with zero timer mutation. If
clear, that same timer is reclassified against the returned owner, followed by
a second latch check. Timer execution is a separate later owner-loop step and
is never an implicit part of reclassification.

The 30 material route cells remain the inventory. Their branch expansion is
now an exact set of 46 authenticated fixtures. Each fixture freezes expected
boundary, returned phase/owner, source PEL disposition, stop point and exact
totals for durable commits, callbacks, publications, P1-d4 revalidations,
XACKs, timer reclassifications and timer executions.

## Reusable I0 regression entrypoint

I0 acceptance runs:

```text
bash scripts/stage8b_p1e_i0_regression_gate.sh ACCEPTED_R9_COMMIT
```

The full commit is mandatory, must resolve exactly and must be an ancestor of
I0 HEAD. The current delta checker permits only the three accepted Rust files,
explicit I0 helper prefixes and the two shared status files. Cargo, workflows,
config, units and deployment remain closed.

The historical `stage8b_p1d4_source_gate.sh` remains byte-unchanged and is not
called by I0 because its historical diff scope is intentionally non-reusable.
The new entrypoint separately verifies immutable P1-d4 design artifacts, calls
the accepted P1-d4 content validator on current I0 source, runs both negative
harnesses and generates all 105 SIGKILL cells in two clean runs with evidence
bound to I0 HEAD/tree. It also runs the full core/service suites, six exact
P1-d2/P1-d3 regression filters, doctests and strict clippy.

R9 opens no I0 implementation yet, no supervisor binary, installation,
operational DB0/VPS, FINAM POST/DELETE, broker dispatch, runtime-live, real
orders, partial fills or protective orders.
