# Stage 6 crash-window and recovery matrix

| Window | Durable owner | Recovery authority | Required outcome |
|---|---|---|---|
| Intent accepted before journal write | Stage 5C/5G paper host | no durable command exists | callback remains uncommitted; no dispatch |
| Journal write before broker dispatch | Stage 6 journal | journal record | safe pending-dispatch state; no invented broker ID |
| Dispatch attempted before broker ID known | Stage 6 journal | attempt record + broker truth | ambiguous; blind retry forbidden |
| Broker accepted but response lost | broker truth | fresh complete order/trade truth | correlate by stable client ID or require reconciliation |
| Broker ID known before local commit | broker evidence | broker truth + pending journal record | append observed broker ID, never overwrite |
| Fill observed before request finalization | broker trade/position truth | Stage 5G convergence | append fill observation then settle lifecycle |
| Cancel requested before cancel result | cancel request record | target order truth | retain cancel pending until terminal/fresh truth |
| Restart with unresolved request | Stage 5D snapshot + Stage 6 journal | Stage 5G restart/reconciliation | restore unresolved state and reconcile before dispatch |
| Duplicate command after restart | journal idempotency identity | exact replay comparison | idempotent no-op; no second dispatch |
| Conflicting command with same idempotency key | journal identity authority | canonical payload digest | terminal fail-closed conflict |

All recovery ordering uses durable sequence and causal linkage. Process time,
receipt wall clock and scheduler timing are diagnostics only. The matrix is
paper/mock design evidence and does not authorize dispatch.
