# Stage 8 slice plan

Status: proposed by Transition Gate 7→8; no Stage 8 implementation is yet
authorized.

## Stage 8A — protected adapter and reconciliation

Stage 8A may begin only after independent acceptance of Transition Gate 7→8.
It implements the broker-neutral protected execution boundary, exact FINAM
MARKET/LIMIT/CANCEL mapping and broker-truth reconciliation using mock/no-send
transport first. It must retain the Stage 7B journal, recovery seal, max-one
lifecycle and Redis settlement authorities unchanged.

Stage 8A does not authorize a real FINAM POST/DELETE. Its exit review must prove
that transport cannot be reached without an opaque, one-shot capability and
that every ambiguous result enters reconciliation without blind retry.

## Stage 8B — bounded real engineering micro

Stage 8B is a separate gate after accepted Stage 8A. It may authorize at most
one explicitly armed engineering command for one account, instrument and
strategy. The initial scope is MARKET, LIMIT or CANCEL only. The exact command,
side, quantity, price bounds and validity window must be approved by the
operator for that run.

Stage 8B requires fresh read-only broker truth before and after the boundary,
an active kill switch, durable attempt evidence before send, response-loss
recovery evidence and independent review. It does not attach an autonomous
strategy runtime.

## Later stages

- Stage 9: continuous order/trade/position reconciliation.
- Stage 10: runtime-live readiness and observability.
- Stage 11: broker shadow parity without dual live ownership.
- Stage 12: controlled runtime-driven FINAM live micro.
- Stage 13: native Stop/SLTP/bracket after dedicated lifecycle contracts.

No later stage is opened by this plan.
