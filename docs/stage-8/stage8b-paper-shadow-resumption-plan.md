# Stage 8B paper-shadow resumption plan

Status: original plan dated 2026-09-02; current-direction addendum 2026-09-30.

## Current-direction addendum — 2026-09-30

The owner confirms that current ALOR systems do not use riskgate. Follow the
[no-riskgate / short-warmup ADR](../adr/adr-stage8b-bo-only-no-riskgate-short-warmup.md)
and [model reconciliation / source plan](stage8b-p1f-bo-only-handoff-alignment-2026-09-30.md).
The immediate target remains baseline07 BO-only paper, without mandatory
High180 shadow accounting, lb120 seed or ledger reconstruction. Paper/shadow
strategy observation is still required; it is not High180 shadow accounting.
The four-session IMOEXF warmup reference is model-specific, not a global limit
for other models. Candidate09 remains separate; no-riskgate does not disable
USDRUBF MR logic.

The installed FINAM binary still has the legacy 121/120-session dependency;
the [additive correction](stage8b-p1f-no-riskgate-source-correction.md) is implemented
and independently SOURCE ACCEPTED at `2f46491`, not installed. The
[authority closure](stage8b-p1f-no-riskgate-governance-closure.md) and fresh CI
are the next boundary. Latest retained O2
ended Failed generation 1 / sequence 4, before bootstrap; its evidence is
prepared, not independently accepted. See [current status](../current-status.md)
for accepted milestones and operational gates. The September 2 milestones and
permissions below are historical context, not fresh deployment authorization.

## Outcome

Resume useful ALOR-to-FINAM parity work without treating the deferred native
installation proof as a paper prerequisite.

## Slice P0: deploy the existing projection

On the isolated VPS deploy:

```text
FINAM WS final M1
  -> isolated Redis finam_imoexf_paper:*
  -> canonical complete M10
  -> paper-only hybrid runtime projection
  -> health/readiness/runtime-state evidence
```

The executable surfaces are:

- `broker-cli finam-ws-shadow-loop`;
- `broker-cli finam-paper-runtime-consume`.

The runtime command must use `--strategy-invocation-shadow` but remains unable
to emit broker commands. ALOR oracle seeding is optional for transport smoke
and required for a claimed state-parity session.

P0 deployment and an isolated Redis DB 15 synthetic M1-to-M10-to-runtime-state
smoke are complete. The smoke is reproducible with
`scripts/stage8b-paper-shadow-db15-smoke.sh`; it does not contact FINAM and
cleans DB 15 on exit.

Live read-only activation is also complete. A separate token whose FINAM token
details report `readonly=true` now drives final M1 bars into the isolated
namespace. Two complete M10 buckets produced committed paper-only runtime-state
batches with consumer `pending=0` and `lag=0`. This proves the live transport
and projection path only: the runtime is unseeded, so ALOR state parity and the
durable paper order/ACK lifecycle are not claimed. See
`stage8b-paper-shadow-readonly-live-activation-evidence.json`.

## Slice P1: deployable durable paper service

The architecture in
`stage8b-p1-durable-paper-lifecycle-composition-design.md` is accepted with
staged authorization. P1-a and P1-b are accepted. P1-c is a source
R1 source implementation review candidate that adds an isolated real-Redis
M10/PEL and idempotent command-publication boundary without operational
activation. R1 also makes restart attachment verify-only and requires
claim-before-fresh Ready acquisition. The dedicated
single-owner composition around the accepted Stage 7B service will add:

- one fixed paper namespace and consumer group;
- file-backed Stage 6/7 recovery ownership;
- an explicit paper-only outcome provider;
- ACK/DLQ/XACK settlement;
- health/readiness publication;
- restart and PEL recovery;
- no FINAM client or transport dependency;
- no broker dispatch capability.

The composition must also source a real Stage 5G clean-restart package and
commitment key and define one owner for semantic continuation and paper
order/trade/position outcomes. A thin CLI around test fixtures or P0 projection
JSON is explicitly insufficient.

This slice requires independent review before VPS activation because it opens
a persistent Redis consumer, even though it remains paper-only.

The independently accepted
`stage8b-p1-semantic-commit-protocol-addendum.md` selects a separate P1
canonical final-M10 stream and requires an authenticated Stage 5G checkpoint
and, for intents, complete Stage 6/7 cross-binding before M10 XACK. The XACK is
always the last semantic commit action.

## Slice P2: multi-session parity

For several sessions compare:

- FINAM-derived and ALOR-native final M10 bars;
- strategy-facing timestamps and session high/low/close;
- owner/cycle/side/pending state;
- paper intents and ALOR live command shape;
- paper ACK/order/trade/position lifecycle;
- restart and gap recovery;
- explicit no-riskgate profile/readiness for the current target. Riskgate
  ledger/state comparison applies only to a separately selected riskgate-enabled
  model, not to baseline07 BO-only admission or paper-session acceptance.

## Pre-live-micro return gate

Before any strategy-driven live micro:

1. add reviewed systemd failed-unit diagnostics;
2. complete the deferred two-run native proof;
3. close full-session parity findings;
4. explicitly issue a separate live-micro authorization.

No paper result implicitly activates Generation 2 or authorizes FINAM order
effects.
