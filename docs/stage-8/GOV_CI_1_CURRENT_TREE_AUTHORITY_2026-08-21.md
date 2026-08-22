# GOV-CI-1B — current-tree CI authority

Status: independent review required. This package changes CI governance only. It does
not authorize Stage 8B-S, FINAM transport, Redis live command consumption, broker
dispatch, runtime-live, or real orders.

## Accepted predecessor

GOV-CI-1A was completed by the explicitly authorized, history-preserving administrative
merge `1dea519cbf2affc3d99866fdae66bbddbafefa24`. The administrative exception was
necessary because the generation-4 Stage-5 authority simultaneously required the old
canonical `ci.yml` and prohibited the only rotation that could retire that requirement.
The evidence is retained in
`docs/stage-8/GOV_CI_1A_ROTATION_DEADLOCK_DISCOVERY_2026-08-21.md` and in the generation-5
Stage-5 retirement artifacts. The historical workflow is now manual-only and has no
`pull_request_target`, `pull_request`, or `push` enforcement.

## GOV-CI-1B decision

The current authority is intentionally split into independent layers:

1. an explicit production Cargo/Rust manifest with path, mode, size, SHA-256 and aggregate
   digest;
2. an explicit governance control-plane manifest binding the canonical workflow,
   historical/manual workflow, current gate, checker, negative harness, handoff tooling,
   contract and inventories;
3. an exact allowlist of active workflow files plus semantic trigger checks;
4. immutable replay of the accepted Stage 8A-5 aggregate gate from
   `bf58b47fdef8af774a4107455dfcc6204e594283`, including its own 20 negatives,
   forbidden-surface 10 negatives, inherited 544 Stage-8 negatives, current I4 28-case
   boundary, Stage-7B durability, external compile-fail tests, debug/release/doctest and
   all-feature clippy;
5. current-tree debug/release/doctest/all-feature-clippy and isolated Redis regression
   smokes;
6. semantic closed-surface checks independent of the generic production fingerprint.

Exact governance hashes make echo, comment, wrapper, alias and command-removal no-ops
fail closed. Semantic negatives deliberately recompute the production manifest after
opening a real default FINAM feature or a closed Stage-6 accessor; those candidates must
still fail.

Operational paper/live configuration is not promoted or frozen here. Its keyed build and
config binding remains part of the later Stage 8B-D R2 design/checker-only package.

## Closed surfaces

- Stage 8B-S authorization: closed;
- FINAM POST/DELETE and broker execution: closed;
- Redis live command consumer and XADD/XACK publication: closed;
- runtime-live and real orders: closed;
- unattended execution: closed.

## Transition rule

GOV-CI-1B must receive independent acceptance before merge to `main`. After acceptance,
Stage 8B-D R2 may start as a separate design/checker-only correction. Only independent
acceptance of that transition package may discuss opening Stage 8B-S.
