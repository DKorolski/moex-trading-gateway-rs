# Stage 2B-5a — explicit order ownership / attribution hardening

Status: implementation patch ready for review.

Date: 2026-07-07.

## What changed

Stage 2B-5a removes implicit runtime ownership from passive order cache updates.

Added:

- `RuntimeOrderAttribution`;
- `observed_order_ids`;
- explicit apply methods:
  - `apply_order_event()` as observed/account-wide default;
  - `apply_owned_order_event()`;
  - `apply_adopted_bootstrap_order_event()`;
  - `apply_order_event_with_attribution(...)`.

Ownership policy:

- `RuntimeOwned`: order is observed and tracked as runtime-owned;
- `AdoptedFromBootstrap`: order is observed and tracked as explicitly adopted;
- `ObservedAccountWide`: order is observed but not runtime-owned;
- `UnknownOrOrphan`: order is observed, not runtime-owned, and emits an
  ownership blocker.

Runtime cache rules:

- `tracked_order_ids()` returns only `owned_order_ids`;
- account-wide or observed broker orders do not silently become strategy-owned;
- orders restored from `known_order_ids` are owned/tracked;
- orders present in `orders` but absent from `known_order_ids` are observed only;
- trades for observed orders are known/cacheable but not strategy-attributed.

## What did not change

- No `HybridIntradayRuntime` behavior changed.
- No BO/MR strategy decision logic changed.
- No implementation-owned `working_orders`, `tp_order_id`, or
  `sl_exchange_order_id` behavior changed.
- No trade ledger implementation changed.
- No command builders changed.
- No real FINAM command consumer was connected.
- No real FINAM `POST`/`DELETE` path was enabled.
- No runtime-live or FINAM `LiveReady` was enabled.
- No Stop/SLTP/bracket/replace/multi-leg live behavior was enabled.
- No RI/RTS or USDRUBF migration was started.
- No `i64` surrogate adapter was introduced.

## Tests added

- `from_validated_state_tracks_known_order_ids_only_not_all_observed_orders`;
- `observed_order_event_does_not_become_tracked_order_id`;
- `unknown_or_orphan_order_event_sets_blocker_and_is_not_owned_even_when_status_known`;
- `adopted_bootstrap_order_event_becomes_tracked_order_id_only_when_explicit`;
- `trade_for_observed_order_is_not_strategy_attributed_without_ownership`.

Updated Stage 2B-5 tests now use explicit owned/adopted attribution where
runtime ownership is intended.

## Remaining live blockers

Stage 2B remains paper/mock/local only:

- runtime-live remains disabled;
- real FINAM command consumer remains disabled;
- strategy-driven real FINAM orders remain disabled;
- Stop/SLTP/bracket/replace/multi-leg live behavior remains disabled;
- RI/RTS and USDRUBF remain out of scope;
- `i64` surrogate adapter remains forbidden without a new ADR.
