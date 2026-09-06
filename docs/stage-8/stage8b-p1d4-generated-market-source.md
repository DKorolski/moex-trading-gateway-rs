# Stage 8B-P1-d4 generated-Market crash/replay source

Status: source implementation review candidate.

Accepted design predecessor:
`1a1ea05775f1d15b86fcc3495ad6863b851e9212` (R7, ACCEPTED).

Accepted business/source predecessor:
`7dc7c802feca6e79d3a1a9902c181ad7b6afc506` (P1-d3, CLOSED / ACCEPTED).

## Scope

This slice implements the exact R7 retained-source generated-Market graph:

```text
later M10 remains pending
  -> Hybrid callback yields one Market intent
  -> reserve exact command Redis ID in combined HMAC package (W0/G0)
  -> persist, fsync, reread and authenticate reservation
  -> explicit-ID XADD plus byte-exact marker in one Lua operation
  -> schedule/provider and Stage 6 V1 order/trade/finalization suffix
  -> combined S_ack (W1/G1)
  -> combined S_truth (W2/G2)
  -> reread and cross-validate package, entry, marker and PEL
  -> XACK the original M10 last
```

The implementation is broker-neutral and paper-only. Tests use disposable,
loopback Redis processes and generated namespaces. Operational Redis DB0/VPS,
FINAM POST/DELETE, broker dispatch, runtime-live, real orders, partial fills,
protective orders and P1-e remain closed.

## Exact publication identity

`Stage8bP1d4CommandPublicationReservationV1` is written into the authenticated
Prepublication package before command publication. It binds the source stream,
group and M10, semantic batch, strategy request, canonical command/envelope,
command stream/group, current command-stream predecessor, its checked immediate
successor and W0.

Only `P1D4_COMMAND_PUBLICATION_LUA` may publish that reserved command. It uses
the explicit Redis ID, never `*`. If the marker already exists, both its bytes
and the exact command entry must match. If it is absent, the reserved ID must
be absent and `last-generated-id` must still equal the authenticated
predecessor. Entry insertion and marker creation are one script operation.
Stream advancement, missing marker/entry asymmetry, payload substitution and
ID repick fail closed.

The versioned marker stores W0 and G0 as canonical decimal strings. Lua/cjson
numbers are not authority. A real Redis regression uses
`G0 = 9_007_199_254_740_993`, proves exact replay, and rejects its neighbouring
generation before schedule, dispatch, provider, ACK, truth or XACK.

## Independent generation chains

Write and covering-seal generations are independent counters:

```text
Prepublication: W0,     G0
S_ack:          W0 + 1, G0 + 1
S_truth:        W0 + 2, G0 + 2
```

No fixed offset between W0 and G0 is accepted. The source proof runs a valid
non-adjacent lifecycle with W0=41 and G0=100 through W1/G1 and W2/G2, and
rejects altered relations.

## Package-aware restart routing

The read-only discriminator is an authenticated tri-state:

- absent: preserve standalone accepted P1-d2 routing;
- present and valid: route the P1-d4 package before ordinary P1-d2;
- declared but invalid: hard corruption, with no P1-d2 or generic fallback.

Valid Prepublication suffixes map to seven finite owners: Prepublication,
DispatchPending, OrderPending, PreFinalizationPending, PreAckPending,
AckCommitted and TruthCommitted. Checkpoint-equal zero suffix remains a P1-d4
Prepublication owner; it is never collapsed to generic Ready.

Each owner exposes only its exact missing continuation. Before S_ack, ACK may
be reconstructed only from the authenticated V1 suffix. After S_ack, only
truth may continue. After S_truth, only source resolution may continue.

## Crash/replay proof

The active registries are immutable R7 inputs:

- 92 base cells from `stage8b-p1d4-scenario-frontier-matrix-v5.csv`;
- 13 generated-Market cells from
  `stage8b-p1d4-generated-market-crash-submatrix-v3.csv`;
- 105 positive real subprocess/SIGKILL cells total.

Every cell also runs a byte-identical duplicate restart and a one-field
commitment-key conflict. The conflict must fail closed as an error or Blocked
owner. Every crash child writes and fsyncs a canonical marker whose audit hash
covers the local durable files before SIGKILL.

Correction C1 is implemented literally. S09/F04 and S09/F09 both restart as
`P1d3PreAckPending`: each has the same recovered CANCEL V3 plus
RequestFinalized suffix without S_cancel_recovered. The volatile in-memory ACK
at F09 is not restart authority. S09/F08 remains the distinct
`P1d3CancelContinuationPending` class.

## Deliberately closed

- current-tree authority rebind before independent source acceptance;
- P1-e deployable supervisor;
- operational Redis DB0 and VPS activation;
- FINAM POST/DELETE and broker network dispatch;
- runtime-live and real orders;
- partial fills, replace, Stop/SLTP, bracket and multi-leg orders.

Acceptance of this candidate may authorize only a governance-only authority
rebind. It does not authorize operational activation.
