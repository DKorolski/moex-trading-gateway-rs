# Stage 8B-P1-e I1 generated-Market process correction

Status: source-correction candidate for the independent HOLD on
`66591fd075898b89187b38880a38a0438a3c0f81`. The independently accepted I1
baseline remains `a655da96ace23eb61d89642f63c49e5275ff98bd`; the intermediate
process checkpoints are accumulated implementation, not independently accepted
boundaries.

## Correction scope

This slice closes only `P1-GMP01`, `P1-GMP02`, and `P2-GMP03` from
`FINAM_I1_GENERATED_MARKET_PROCESS_REVIEW_66591fd_2026-09-17.md`.

### GMP01 — reservation-bearing generated publication

The process row now classifies a prepublication owner from its authenticated
durable candidate before consuming it. A generated-Market candidate calls the
existing reservation/seal publisher; all other supported candidates retain the
generic publisher. The generic publisher still rejects generated-Market
candidates, so there is no second or fallback publication path.

The process-level test starts from a real Working lifecycle outcome, traverses
the ordinary drain/dispatcher, observes exactly one generated publication,
waits for the successor, rereads a revised signed schedule, and completes
`V4 -> combined S_ack -> S_truth -> source XACK-last`. It also proves no second
Hybrid callback or command publication.

### GMP02 — typed successor waiting

Plain Market, generated Market, and initial LIMIT now distinguish an absent
future successor from an invalid durable source. A bounded lookup that sees no
successor or a transient Redis/timeout result returns the same linear published
owner in `AwaitingSuccessor`; the process exposes that owner through the
existing schedule-deferred boundary.

Each retry performs a fresh signed-schedule read before effect authority can be
issued. The previous schedule snapshot and high-water are not committed while
waiting. Shutdown is checked through the existing bounded schedule policy and
retains the source for authenticated restart.

An empty successor range triggers an exact predecessor reread. A missing
predecessor therefore remains terminal instead of becoming an infinite wait.
Wrong successor ID, non-contiguous close time, payload identity mismatch and
other exact-source conflicts remain fail closed.

### GMP03 — reusable I1A regression tooling

The I1A source checker now verifies the explicit Initial/Working/Cancel route
partition and its three legacy-compatible rejection branches. The negative
harness runs a positive baseline and a no-op control before 102 independent
mutations; it no longer depends on the obsolete Cancel-only source text.

## Evidence boundary

The correction evidence uses `--all-features` for runtime tests. It retains the
full selected test names and non-zero pass counts for generated-Market process
coverage, signed-V4 restart after `S_ack` and `S_truth`, plain Market and
initial-LIMIT regressions, the complete runtime/service suites, doctests,
strict Clippy, the 60-case P1-d4 harness and the 102-case I1A harness.

No subprocess failure was observed during this correction run. The earlier
timing failure mentioned in review did not have a retained raw log and is not
represented as reproduced or diagnosed evidence. A future process-level
signal/panic/SIGKILL matrix remains a separate gate.

## Deliberately closed

- Cancel and Day-expiry signed-schedule composition;
- production `run` wiring and process crash/signal acceptance;
- operational Redis DB0/DB15 or VPS activation;
- FINAM POST/DELETE/send, broker dispatch, runtime-live and real orders.

This document records a review candidate. It does not claim independent
acceptance and does not open any operational surface.
