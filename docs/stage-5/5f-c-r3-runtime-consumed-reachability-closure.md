# Stage 5F-c R3 — runtime-consumed reachability closure

R3 closes the two remaining R2 reachability defects without opening Stage 5F-d or any live surface.

## F19 paired counterfactual

The owner fixture has `was_long_today=false`, a valid range, neutral big-move history, a post-wait M10 bar and a strict BO long threshold crossing. A Rust paired test holds config, riskgate, bar, clocks and broker truth constant. Its flat control emits exactly one BO buy; the MR-owned/open state emits zero intents.

## F26 runtime-consumed broker truth

F26 carries a canonical working `BrokerOrderSnapshot`. The typed harness validates and transfers it through runtime `working_orders`, `Stage5dExpectedWorkingSets`, canonical `known_order_ids`, Stage 5C admission broker truth, Stage 5D bootstrap and restored runtime state. The callback occurs with a pending age greater than 60 seconds; it emits no intent and retains the exact pending request, active cycle and working order ID.

## F12–F15

These rows are favorable/adverse extreme invariants only. Actual target/stop completion remains owned by the future Stage 5G feedback lifecycle.

## Closed boundary

Redis, FINAM transport, POST/DELETE, dispatch, broker execution, runtime-live, protective completion and Stage 5F-d remain closed. Independent acceptance of this R3 package is required before Stage 5F-d.
