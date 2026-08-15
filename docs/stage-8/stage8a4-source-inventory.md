# Stage 8A-4 reconciliation source inventory

## Accepted sources that may be composed

- `broker_core::BrokerTruthSnapshot` and its order/trade/position/instrument
  rows are the canonical normalized model.
- `broker_finam::mapper::map_finam_broker_truth_snapshot_with_readonly_artifacts`
  is the existing FINAM-to-canonical mapper.
- `FinamStage4ReadonlySourceEvidence` and the Stage 4 source-status/freshness
  model are implementation inputs to inspect and adapt, not automatic Stage 8
  authority.
- Stage 6/7 durable request, client-order, broker-order, journal generation and
  recovery seal are the identity/restart authority.
- Stage 8A-3 provides endpoint-specific classification context but not broker
  truth.

## Historical implementations that are oracle-only

The `CancelBrokerTruth*`, `map_cancel_broker_truth_*`,
`reconcile_cancel_broker_truth_sources`, M3d2 transport lifecycle and older
real-readonly reconciliation helpers in `finam-gateway/src/lib.rs` are not
Stage 8A-4 authority. They are useful fixture/oracle inventory only.

They cannot be reused directly because, among other differences, historical
logic can classify position evidence as terminal, exposes serializable public
decision/report types, uses source precedence rather than the new exact
correlation tiers and is coupled to older endpoint/lifecycle surfaces.

## Required implementation gaps

- opaque Stage 8A-4 request context bound to current durable identity;
- explicit completeness/pagination/source-interval evidence;
- sealed freshness and bounded-event policy authority;
- exact canonical instrument resolution using FINAM venue identity;
- deterministic tiered order candidate selection;
- trade support and contradiction rules;
- account-wide safety-guard summary separated from target lifecycle truth;
- outcome algebra without `ProvenNoMatch`;
- crate-private durable application bridge with crash/restart invariants;
- Stage 8-specific scanners and fixture-backed negative coverage.

No production source is changed by this inventory/design package.
