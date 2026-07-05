# M4-2h instrument identity closure / real-readonly canonical package

Status: implementation hardening, GET-only evidence path. No new live orders are authorized by this step.

M4-2h closes the remaining instrument-identity gap found after M4-2g-a: broker truth must not infer safety from a local row count or from a ticker-only match. Canonical order/trade truth now has enough broker-neutral identity to distinguish instruments that can share the same visible venue symbol.

## Policy decisions

1. Empty instrument registry is not silently accepted.

   If an order carries a venue symbol but `BrokerTruthSnapshot.instruments` is empty, the order receives `BrokerOrderOrphanReason::MissingInstrumentRegistry`.

2. Unknown venue identity remains distinct.

   If the order has no usable venue symbol, the reason remains `BrokerOrderOrphanReason::UnknownInstrumentIdentity`.

3. Same-symbol ambiguity is explicit.

   If more than one instrument spec matches the order's base identity and the order does not carry enough broker-native fields to disambiguate, the order receives `BrokerOrderOrphanReason::AmbiguousInstrumentIdentity`.

4. Order/trade snapshots now carry optional enriched identity fields:

   - `broker_asset_id`;
   - `board`;
   - `expiration_date`.

5. FINAM mapper enriches order/trade identity only when exactly one instrument spec matches the order/trade base `InstrumentId`.

   If there are zero matches or multiple matches, enrichment is skipped and the canonical orphan/ambiguity policy decides safety from `BrokerTruthSnapshot`.

6. FINAM instrument spec tolerates missing `asset.future_details.step_price`.

   If FINAM omits explicit step value, mapper derives it from `price_step * contract_size`, with lot-size fallback. This keeps real read-only canonical package construction from failing on an otherwise valid futures spec while still preserving deterministic instrument economics.

## Canonical real-readonly package

`finam-typed-readonly-check` now emits an additional redacted record:

```text
canonical_readiness_package_typed
```

The record is built from typed FINAM GET-only artifacts through `FinamCanonicalReadinessPackage`:

- account snapshot;
- orders snapshot;
- trades snapshot when the read-only trades endpoint succeeds;
- quote;
- asset;
- asset params;
- schedule.

The emitted record exposes only summary fields:

- `truth_source = BrokerTruthSnapshot`;
- `readiness_source = BrokerReadinessSnapshot`;
- `package_source = FinamCanonicalReadinessPackage`;
- order/position/trade/instrument counts;
- enriched order/trade counts;
- canonical orphan/active-order summary;
- canonical preflight block count;
- `no_live_authorization`;
- `order_endpoints_used = false`.

No account id, token, raw body, raw order id, raw trade id, raw position row, or endpoint response body is written by this canonical summary.

## Trading boundary

M4-2h does not authorize:

- real POST `/orders`;
- real DELETE `/orders/{id}`;
- command-consumer-to-real-FINAM;
- continuous runtime-live;
- new live position tests;
- Stop/SLTP/bracket/replace/multi-leg.

The only broker-side action expected for M4-2h evidence is redacted FINAM read-only GET probing through existing read-only CLI paths.

## Acceptance evidence

The evidence script is:

```text
scripts/m4_2h_instrument_identity_readonly_package_evidence.py
```

It checks:

- new orphan reasons and identity fields are present;
- same-symbol ambiguity tests exist;
- FINAM mapper enriches order/trade identity from instrument specs;
- typed CLI has `canonical_readiness_package_typed`;
- forbidden order endpoint scanners remain green;
- optional real read-only report contains a successful canonical package record with `order_endpoints_used = false`.

Live expansion remains blocked after M4-2h.
