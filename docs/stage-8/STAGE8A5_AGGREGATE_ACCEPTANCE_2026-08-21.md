# Stage 8A-5 aggregate acceptance

Status: aggregate acceptance candidate; independent acceptance pending.

## Scope

Stage 8A-5 adds no functional implementation. It aggregates and reruns the
accepted Stage 7B and Stage 8A contracts on top of independently accepted I4
Implementation R3 `4a11688c941ee240e377b384042c4bca837b040f`.

The accepted I4 review SHA-256 is
`0377879b5b10c38ef0740af54e3d2b341d980b21824490664d828e8a6d4e0046`.

## Aggregate gate

The gate performs:

1. exact lineage and governance validation;
2. detached rerun of accepted Stage 7B aggregate durability;
3. detached semantic and negative reruns for Stage 8A-0 through accepted
   Stage 8A-4/I4 slices;
4. current I4 semantic/negative and external compile-boundary checks;
5. Stage8-specific forbidden-surface scan and mutation harness;
6. workspace debug and release tests, doctests, fmt and clippy;
7. commit-bound source/evidence manifest and archive safety verification.

The accepted Stage 7B X16 subprocess witness has a fixed two-second claim
barrier. On a cold recursive build the aggregate orchestrator permits one
bounded rerun of the whole affected gate or workspace command only when the
first run has exactly one failed test, that test is X16, and its exact failure
is `X16 child did not reach claim barrier`. No test is skipped: the second run
must complete the entire accepted Stage 7B gate or workspace command
successfully. Any other failure or a second failure blocks 8A-5.
The inherited Stage 7B gate runs with `RUST_TEST_THREADS=1`, and the current
workspace debug/release commands use `--test-threads=1`, so X16 does not
compete with the other active Redis subprocess tests in the same binary.
This changes scheduling only: every inherited and current workspace test still
executes, and the exact-failure bounded retry remains fail-closed.

The historical repository-wide `forbidden_surface_scan.sh` remains an immutable
Stage 5 gate whose workspace/Cargo allowlist predates accepted Stage 6–8
crates. Stage 8A-5 does not rewrite it. Instead it uses a Stage8-specific
scanner that requires an empty production/Cargo/workflow delta from accepted
I4 and scans all Stage8 production modules for HTTP order transport, Redis
mutation and dispatch surfaces.

## Closed surfaces

Stage 8A-5 keeps closed:

- ACK/readiness publication;
- Redis XADD/XACK and live command consumption;
- FINAM POST/DELETE and network order transport;
- broker dispatch and retry/resend/re-arm;
- runtime-live and real strategy orders;
- Stage 8B.

Only exact independent `ACCEPTED` may close Stage 8A and authorize a separate
Stage 8B design package for one bounded engineering micro. It does not itself
authorize that micro.
