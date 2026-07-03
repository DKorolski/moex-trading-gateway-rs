# M3d-1 FINAM contract alignment before real order endpoint

Status: next accepted work item after the 2026-07-03 ALOR-to-FINAM audit v2.
This stage is a hard P0 prerequisite before executable FINAM order `POST` /
`DELETE` source is added.

M3d-1 does not enable live trading, does not make `EndpointGateApproved`
constructible, does not enable the order endpoint scanner allowlist mode, and
does not attach runtime strategies.

## Goal

Close FINAM contract drift risks before any real order endpoint implementation:

1. `TimeInForce` mapping.
2. Explicit `OrderStatus` classifier.
3. `InstrumentRegistryValidator` as a `LiveReady` blocker.
4. Pinned FINAM enum/status/spec fixtures.
5. Drift tests and evidence binding.

Current implementation anchors:

- `crates/broker-finam/src/order_request.rs` contains
  `finam_time_in_force()` and currently needs M3d-1 alignment before any
  executable endpoint transport is added.
- `crates/broker-finam/src/mapper.rs` contains `map_order_status()` /
  `classify_order_status()` and currently needs explicit production status
  buckets.
- `crates/broker-core/src/readiness.rs` already contains
  `InstrumentMapNotValidated` and `ScheduleNotLoaded`; M3d-1 should wire the
  validator into these readiness blockers instead of inventing a parallel
  readiness vocabulary.

## M3d-1.1 TimeInForce mapper

Expected policy:

```text
Day                -> TIME_IN_FORCE_DAY
GoodTillCancel     -> TIME_IN_FORCE_GOOD_TILL_CANCEL, only if verified
ImmediateOrCancel  -> TIME_IN_FORCE_IOC
FillOrKill         -> TIME_IN_FORCE_FOK
GoodTillDate       -> block until explicit ValidBefore support is reviewed
Unsupported        -> LocalRejected before broker call
```

Acceptance:

- unsupported TIF never reaches an HTTP body;
- IOC/FOK serialize as exact FINAM enum values;
- `GoodTillDate` is blocked unless `ValidBefore` support is explicitly
  implemented and tested;
- mapper tests fail on unknown enum/string drift.

## M3d-1.2 OrderStatus classifier

The classifier must be explicit. Production statuses must not silently collapse
into a harmless `Unknown`.

Target buckets:

```text
terminal_rejected:
  FAILED
  DENIED_BY_BROKER
  REJECTED_BY_EXCHANGE

active_or_pending:
  WAIT
  FORWARDING
  WATCHING
  PENDING_NEW

cancel_pending:
  PENDING_CANCEL

terminal_filled:
  EXECUTED
  SL_EXECUTED
  TP_EXECUTED

needs_policy:
  DONE_FOR_DAY
  REPLACED

manual_or_degraded:
  SUSPENDED
  DISABLED

unknown:
  block LiveReady + DLQ/manual review
```

Acceptance:

- unknown production status blocks `LiveReady`;
- terminal statuses update order path deterministically;
- pending statuses do not create terminal ACK;
- cancel pending is not treated as canceled;
- policy statuses have explicit decisions instead of silent fallback.

## M3d-1.3 InstrumentRegistryValidator

Validate every configured trading instrument before any future endpoint enable:

- symbol and MIC;
- FINAM reference data: assets, asset params, schedule;
- price step, quantity step, lot/min quantity;
- futures expiry / roll metadata;
- account/instrument availability;
- session/tradability;
- currency and market compatibility.

Blocking reasons include:

```text
unknown_symbol
missing_mic
missing_price_step
price_step_mismatch
missing_qty_step
qty_step_mismatch
missing_lot_size
expired_contract
not_tradable
schedule_missing
session_closed
currency_mismatch
manual_override_required
```

Acceptance:

- `LiveReady` is impossible without `InstrumentMapValidated`;
- `ScheduleLoaded` is required before any future order endpoint call;
- wrong tick/quantity/lot is rejected locally;
- futures expiry/roll blocks until operator acknowledgement;
- USDRUBF, IMOEXF, and RI/RTS resolve independently; RI/RTS remain last in
  migration order.

## M3d-1.4 Pinned spec fixtures and drift harness

Work:

- pin FINAM enum/status/body-shape fixtures under test fixtures;
- add mapper golden tests against pinned fixtures;
- add drift checks that fail on unreviewed enum/status/body drift;
- attach fixture hashes to future evidence packages.

Acceptance:

- DTO mapper cannot silently drift;
- unsupported enum becomes local reject or degraded/manual review, not live
  pass-through;
- evidence package shows fixture version/hash.

## Definition of done

```bash
cargo fmt --all --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/forbidden_surface_scan.sh
bash scripts/forbidden_surface_negative_harness.sh
bash scripts/order_endpoint_scanner_transition_spec.sh
bash scripts/redis_shadow_smoke.sh
bash scripts/runtime_bridge_dry_smoke.sh
```

No real order endpoint calls may be added in M3d-1.
