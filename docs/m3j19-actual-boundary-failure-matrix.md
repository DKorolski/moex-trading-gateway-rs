# M3j-19 actual boundary failure matrix

Date: 2026-07-04

## Goal

M3j-19 hardens the actual order boundary after the first closed M3j live-micro
LimitCancel milestone. It does not send broker orders and does not expand live
scope.

The stage records how the current protected order boundary handles ambiguous
and failure outcomes before any additional controlled live micro run.

## Boundary

M3j-19 keeps these boundaries closed:

- no new live order send;
- no continuous runtime-live attachment;
- no command-consumer-to-real-FINAM;
- no Stop/SLTP/bracket/replace/multi-leg;
- no portfolio-level live execution;
- no blind retry after ambiguous place/cancel outcomes.

## Required failure matrix

| Case | Expected handling |
| --- | --- |
| place accepted, broker order id missing | `SubmittedPendingBrokerOrderIdReconciliation`; no blind retry; reconciliation required |
| place timeout after send | `TimeoutUnknownPending`; no blind retry; reconciliation required |
| place HTTP 4xx/5xx | reject/disarm/manual-intervention policy according to classifier/lifecycle |
| place send error after possible send | `ReconciliationRequired`; no blind retry |
| cancel accepted | `CancelAcceptedPendingReconciliation`; post-run reconciliation required |
| cancel timeout after send | `CancelTimeoutUnknownPending`; no blind retry |
| cancel rejected/not found/already terminal | conservative terminal/conflict handling; no blind retry |
| duplicate actual invocation | blocked by one-shot marker / checkpoint reuse guard |
| retry after ambiguous place | blocked until broker-truth reconciliation or explicit operator decision |

## Evidence sources

The M3j-19 evidence bundle binds this matrix to:

- transport post-send semantics tests;
- lifecycle matrix tests;
- checkpoint marker reuse tests;
- command-consumer default-disabled tests;
- forbidden surface scanners;
- the frozen M3j-18 release-freeze bundle.

## Next stage

Only after M3j-19 review should the project consider M3j-20, a second
controlled live LimitCancel run that explicitly observes a working/active broker
order before cancel. M3j-20 requires fresh explicit operator approval.
