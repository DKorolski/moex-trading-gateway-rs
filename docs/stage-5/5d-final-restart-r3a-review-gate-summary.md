# Stage 5D-final-restart-r3a — review gate summary

Status: superseded by Stage 5D-final-restart-r3a-r1 review candidate, no-I/O.

Stage 5D-final-restart-r3a answers the restore-ownership question raised by the
r3 discovery review.

## Verdict from executable evidence

The defect is not owned by raw `HybridIntradayRuntimeStrategy::set_state(...)`.
That function may reconstruct a generic semantic placeholder. The exact pending
entry shape is owned by the Stage 5D runtime-private extension and is restored
by `stage5d_apply_runtime_private_extension(...)` before broker bootstrap,
riskgate injection, runtime-state-restored callback and Stage 5C continuation.

Therefore no source correction to `hybrid_intraday_runtime.rs` is included in
this handoff.

## Added executable matrix

Focused positive test:

```text
stage5d_final_r3a_source_pending_entry_full_restart_matrix
```

It covers:

- MR Long bracket pending entry;
- MR Short bracket pending entry;
- BO Long market pending entry;
- BO Short market pending entry.

Each case is source-produced through `on_bar(...)`, exported into a canonical
Stage 5D restart package, serialized to strict package bytes, decoded into a
fresh runtime, restored through private apply, bootstrapped, riskgate-injected,
notified as runtime-state-restored exactly once and continued into Stage 5C
history warmup.

Focused negative test:

```text
stage5d_final_r3a_source_pending_package_negatives_fail_closed
```

It proves through the canonical package boundary that:

- incomplete MR stop/take fails closed;
- owner/side/reason mismatch fails closed.

## Boundary notes

The existing `runtime_state_restored` callback clears flat/no-working-order
pending tails as boot-stale cleanup. r3a intentionally asserts the exact pending
shape immediately after private apply and before the restored callback. Active
working-order ownership remains closed by Stage 5C/Stage 5D policy and is not
opened here.

Closed surfaces remain closed:

- Redis;
- FINAM;
- broker transport;
- dispatch/send/publish sinks;
- runtime-live;
- broker execution;
- real endpoints or live orders.

## Checker binding

The Stage 5D additive freeze checker is advanced to
`5D-final-restart-r3a-r1` in the follow-up closure and marker-pins the new
reproduction tests, post-apply semantic/private equality proof and
private-apply-before-callback ordering. The immutable Stage 5C closure baseline
is unchanged.

## Next step

After r3a acceptance, resume Stage 5D-final-restart-r3:

- full executable positive package matrix;
- real durable crash/checkpoint simulator;
- full package-negative matrix;
- checked-in golden vectors;
- inventory rows tied to executable evidence.
