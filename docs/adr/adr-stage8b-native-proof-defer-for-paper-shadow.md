# ADR: defer Stage 8B native installation proof while paper-shadow proceeds

Status: accepted operator decision, 2026-09-02.

## Context

The accepted R0-R2B verifier repair was exercised on the isolated native
Linux/amd64 VPS. Archive and host preflight passed, and the pinned verifier
proved all 13 signing-key bindings plus the account-key binding. The following
systemd transaction stopped before run 1 with a dependency-job failure. Both
inner and outer cleanup proved that the ceremony source and proof container
were removed, uninstall passed, Generation 2 stayed inactive and authorization
stayed `NOT_ISSUED`.

The native transaction is a production installation/trust proof. It does not
validate market-data parity, strategy semantics or the paper lifecycle. A
third private-material retry without a safe failed-unit diagnostic would add
risk and delay without advancing those product properties.

## Decision

The native transaction proof is deferred to the pre-live-micro gate. No new
private ceremony transfer is allowed until a separately reviewed diagnostic
patch captures the exact failed systemd unit and redacted journal state before
cleanup.

Paper-shadow work may proceed on the same VPS because it:

- uses isolated `finam_imoexf_paper:*` Redis namespaces;
- consumes FINAM market data only;
- produces canonical M10, paper state, paper intents and paper ACKs only;
- keeps command-consumer-to-real-FINAM disabled;
- cannot publish `LiveReady`;
- cannot call FINAM order POST/DELETE;
- does not consume or require the Generation-2 private ceremony.

## Deployment truth

The existing `broker-cli finam-paper-runtime-consume` command is deployable for
WS M1 to canonical M10 and paper runtime projection. The accepted Stage 7B
durable Redis command service is currently a library/test composition: there
is no production executable and no production `Stage7aPaperOutcomeProvider`
composition in the repository. It must not be represented as already deployed.

Therefore the operational sequence is:

1. deploy the existing read-only WS/M10/runtime projection;
2. collect session evidence and ALOR comparisons;
3. implement and review a dedicated paper-only Stage 7B service binary;
4. run multiple paper sessions;
5. return to the deferred native installation diagnostic before live micro.

## Unchanged boundary

Generation 2 remains inactive and R2B remains `NOT_ISSUED`. FINAM POST/DELETE,
broker dispatch, Redis-to-real-FINAM consumption, runtime-live and real orders
remain forbidden.
