# Stage 5C-n - bounded deterministic paper-loop coordinator

Status: review candidate. Date: 2026-07-13.

This facade is a small orchestration layer over the accepted Stage 5C
type-states. It does not add strategy logic and does not call strategy
callbacks directly. Every state transition delegates to an already accepted
Stage 5C facade.

The coordinator is intentionally not an autonomous runtime loop. One call to
`advance_stage5c_paper_loop_once(...)` consumes one state and one explicit
event, then returns one next state or a fail-closed error.

Accepted input state kinds:

- pending recovered paper strategy;
- semantic bar result;
- settled paper intent batch;
- ACK-resolved intent batch;
- broker-lifecycle-resolved batch;
- broker-lifecycle settlement;
- timer-resolved strategy;
- opaque timer settlement.

Accepted event kinds:

- one final M10 semantic bar;
- explicit semantic-result settlement;
- one ACK lifecycle input;
- one full broker-lifecycle event batch;
- explicit broker-lifecycle-result settlement;
- one timer input;
- explicit timer-result settlement.

Coordinator gates:

- invalid state/event pairs are blocked and preserve the input state;
- broker events enter only as a full Stage 5C-j batch, preserving atomic
  preflight semantics for multi-request and multi-event lifecycles;
- final bars, ACKs, broker batches and timers are routed only through existing
  Stage 5C functions;
- callback-generated broker intents are settled into an opaque generated batch
  and must re-enter the ACK lifecycle before timer or bar continuation;
- generated timer batches can re-enter the ACK lifecycle only through the
  generated-batch extractor accepted in Stage 5C-m;
- recoverable timer blocks preserve the broker-lifecycle-resolved state exposed
  by Stage 5C-k;
- ready timer settlements cannot be unwrapped or ACK-resolved;
- recoverable blocks preserve their corresponding type-state when the
  underlying facade exposes it;
- every public state reports no intent sink, Redis command stream or broker
  transport attachment.

Still closed:

- autonomous/infinite runtime loop;
- Redis consumer groups;
- intent sink;
- broker transport;
- FINAM command consumer;
- real order endpoints;
- runtime-live attachment;
- stop/SLTP/bracket execution.
