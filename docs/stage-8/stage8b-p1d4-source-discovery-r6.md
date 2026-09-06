# Stage 8B-P1-d4 source discovery R6

Status: immutable committed-source evidence for a design-only correction.

The R5 review found three source/design mismatches. Inspection is pinned to
the committed R5 parent `b377c0275f1ce5f01cfe9b223724bf1542f985e2`, not to
the saved dirty source WIP.

## Accepted Market shape

P1-d2 Market uses Stage6 V1 records:

```text
RequestAccepted
DispatchAttemptRecorded
BrokerOrderObserved
BrokerTradeObserved
RequestFinalized(Completed)
```

`execute_stage6d_paper_outcome()` builds order and trade separately and the
writer appends and refreshes after each record. Dispatch-only and
dispatch+order are therefore real durable crash frontiers. The existing
P1-d2 journal-ahead classifier accepts only suffix lengths 3 or 4 and starts
with `[dispatch, order, trade, rest @ ..]`; R6 must add a narrower P1-d4
partial classifier rather than changing the accepted classifier or inventing
a V3 Market record.

The deterministic provider is entered only after a durable dispatch receipt.
The accepted reconstruction function derives exact Market evidence from the
authenticated decision and canonical M10 without reacquiring schedule
authority. This is sufficient for a dispatch-only continuation. An order-only
continuation additionally needs a narrow append-missing-trade path; calling the
existing all-record outcome writer unchanged would duplicate the order.

## Accepted sequence shape

`RequestFinalized` is appended and reread before entering the ACK stage. The
sole sequence allocator is then called inside
`apply_stage8b_p1d2_ack_stage()`, followed immediately by the fsync-backed
test marker and `p1d2-after-sequence-pair-before-ack` barrier. Therefore no
pair exists before finalization, and RequestFinalized-before-allocator is a
distinct durable frontier.

## Accepted publication shape

The real Redis publication marker and receipt jointly identify the exact
source stream/group/id, semantic batch, request, command bytes, canonical
envelope, command stream/group/entry and prepublication seal. The receipt is
process-local. Replacement `S_ack` and `S_truth` cannot revalidate Redis after
restart unless that canonical identity is persisted and HMAC-covered. R6
therefore adds `Stage8bP1d4CommandPublicationBindingV1`; it serializes identity
only and never a Redis or effect capability.

The exact committed-source SHA-256 values and mandatory shape assertions are
recorded in `stage8b-p1d4-source-shape-r6.json` and enforced by the R6 checker
even when its artifact-hash mode is disabled by the negative harness.

The saved uncommitted source work remains excluded from R6 and is not evidence.
