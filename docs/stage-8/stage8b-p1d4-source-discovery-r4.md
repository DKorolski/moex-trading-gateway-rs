# Stage 8B-P1-d4 source discovery R4

Status: design correction required before the P1-d4 source candidate may be
committed.

Date: 2026-09-06.

Accepted design predecessor:
`e1ce6d3baec3974d8dfd05c2f3de00110e0605bf` (P1-d4 R3 design,
CLOSED / ACCEPTED).

The authorized implementation attempt reached the exact subprocess/SIGKILL
matrix and exposed two contradictions in the frozen v3 cell registry. This is
a design discovery, not permission to infer crash phase from volatile test
state or to widen production persistence.

## Discovery D1: S09/F04 and S09/F09 are durably indistinguishable

The accepted registry requires:

```text
P1D4C-092 S09/F04 -> P1d3PreAckPending
P1D4C-064 S09/F09 -> P1d3CancelContinuationPending
```

At both restart points the durable state is exactly:

```text
target LaterFilled V3
target truth
target S_terminal persisted and reread
recovered CANCEL V3
recovered CANCEL RequestFinalized
no S_cancel_recovered
source command M10 still pending
```

F09 additionally applied the recovered ACK only in process memory. SIGKILL
destroys that volatile fact. Therefore a deterministic restart classifier
cannot distinguish F04 from F09 using authenticated durable evidence.

The rejected implementation alternative was to consult the crash-marker file
or test environment from production recovery. The marker is test evidence,
not runtime authority. That alternative would make recovery depend on the
test harness and is forbidden.

The narrow correction maps P1D4C-064 to `P1d3PreAckPending`, the same durable
equivalence class as P1D4C-092. Exact recovered-ACK replay followed by
`S_cancel_recovered` remains the sole continuation. The intermediate
`P1d3CancelContinuationPending` owner remains required only after target
`S_terminal` and before the recovered CANCEL V3/RequestFinalized suffix.

## Discovery D2: S05/F15-F16 incorrectly couple two source lifecycles

The accepted frontier definition already says:

```text
F15 callback and any one-intent publication durable before bar XACK
    -> exact bar XACK only; callback/publication replay forbidden
```

However P1D4C-039 additionally required the complete generated command
lifecycle before acknowledging the originating bar, and P1D4C-040 expected a
`P1d3TruthCommitted` restart owner.

The S05 Hybrid callback produces a P1-d2 Market command. Once its exact
publication is durable, that command has its own M10 source and restart
lifecycle. The later-bar source no longer owns the Stage5C settlement
authority needed to settle the generated Market command inside the P1-d3
replacement package. Attempting that composition fails closed with
`RestartRuntimeRequired` before the F16 crash barrier.

The narrow correction preserves source independence:

```text
later-bar source:
  callback once -> exact publication durable -> exact bar XACK

generated command source:
  P1SemanticPrepublicationReady -> accepted P1-d2 lifecycle independently
```

After F16, absence from the exact bar PEL and the authenticated group frontier
prove `AlreadyAcknowledged` for the bar. The remaining generated command is
recovered as `P1SemanticPrepublicationReady`; callback and publication are not
replayed.

## Minimal reproduction from the implementation worktree

The implementation worktree itself is deliberately excluded from this
design-only correction commit. The two focused commands that exposed the
contract conflict were:

```text
STAGE8B_P1D4_DEBUG_CELL=P1D4C-091 \
cargo test -p runtime-durable-service --all-features \
  p1d4_exact_registry_cells_sigkill_to_frozen_restart_disposition -- --nocapture

observed restart: P1d3CancelContinuationPending
frozen expectation: P1d3PreAckPending
```

and:

```text
STAGE8B_P1D4_DEBUG_CELL=P1D4C-040 \
cargo test -p runtime-durable-service --all-features \
  p1d4_exact_registry_cells_sigkill_to_frozen_restart_disposition -- --nocapture

observed before barrier: Durable(Runtime(RestartRuntimeRequired))
```

These results are discovery evidence only. Source acceptance still requires a
clean immutable implementation commit and the complete corrected 92-cell run.

## Scope preserved

R4 does not authorize a new journal record, schema version, runtime owner,
provider call, dispatch, operational Redis DB0/VPS activation, P1-e paper
supervisor, FINAM POST/DELETE, runtime-live or real orders.

