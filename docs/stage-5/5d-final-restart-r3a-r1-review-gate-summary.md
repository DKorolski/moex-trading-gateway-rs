# Stage 5D-final-restart-r3a-r1 — review gate summary

Status: review candidate, no-I/O.

Stage 5D-final-restart-r3a-r1 closes the two P1 findings from independent
review of `fbb526f` without patching `HybridIntradayRuntimeStrategy::set_state`.

## What changed

The source-produced MR/BO pending-entry restart matrix now proves the exact
post-apply state from the actual fresh runtime:

```text
source on_bar
-> canonical package export
-> strict serialize/decode
-> fresh runtime semantic set_state
-> placeholder is rejected as final shape
-> runtime-private apply
-> exact actual Strategy::state pending lifecycle equality
-> exact runtime-private DTO equality
-> broker bootstrap
-> riskgate injection
-> restored callback once
-> Stage 5C warmup continuation
```

The exact semantic fields compared after private apply and before broker
bootstrap are:

- `pending_entry_owner`;
- `pending_entry_side`;
- `pending_entry_cycle_id`;
- `pending_entry_request_id`;
- `pending_entry_created_ts_utc`.

The exact private DTO comparison covers owner, side, reason, entry style,
request id, target quantity, stop/take and the explicit absence of a
partial-entry timer for all four non-partial cases.

## Negative evidence

The Stage 5D negative harness inventory is extended to 123 cases. The r3a-r1
marker-pinned cases cover:

- reproduction test removal;
- post-apply private equality removal;
- post-apply semantic equality removal;
- restored callback ordering drift;
- MR long/short reason mapping drift;
- BO reason mapping drift;
- MR stop/take shape loss;
- incomplete MR package acceptance;
- owner/side/reason mismatch acceptance;
- unauthorized `set_state(...)` source mutation.

The harness still rejects an unexpected pass, a missing exact marker and Python
traceback/infrastructure failure as semantic success.

## Boundary

Closed surfaces remain closed:

- Redis;
- FINAM;
- transport and command consumer;
- dispatch/send/publish sinks;
- runtime-live;
- broker execution;
- real endpoints or live orders.

The immutable Stage 5C closure baseline and ALOR/source oracle remain unchanged.

## Reproduced gate evidence

Local review gate log:

```text
reports/stage-5/stage5d-final-restart-r3a-r1-review-gate.log
lines: 1699
sha256: a62cf4a7949d4e97fb717a7dcec4152f7a507d07a5941b01777634dd71dd1e60
```

Toolchain:

```text
rustc 1.95.0 (59807616e 2026-04-14)
cargo 1.95.0 (f2d3ce0bd 2026-03-21)
```

## Next step

After r3a-r1 acceptance, resume Stage 5D-final-restart-r3 full closure:

- full executable positive package matrix;
- real durable crash/checkpoint simulator;
- full package-negative matrix;
- checked-in pinned golden vectors.
