# Stage 5D-final-restart-r3 aggregate closure r2

Status: review candidate, evidence/governance-only re-closure.

This document supersedes the earlier Stage 5D final-restart closure and
resumption notes as the authoritative current Stage 5D status. Those older
documents are retained as historical development records.

Stage 5D aggregate closure r2 does not change frozen runtime semantics. It
repairs the formal evidence chain for the already reviewed no-I/O persistence,
restart and recovery-semantics slice.

## Closure claim

- mandatory positive inventory: 21 cases;
- accepted executable cases: 21;
- TODO source-produced cases: 0;
- Stage 5C continuation remains required;
- Stage 5E remains closed;
- Redis, FINAM, broker transport, dispatch, runtime-live and broker execution
  remain closed.

## Evidence repair

The r2 closure introduces one top-level runner:

```text
scripts/stage5d_final_restart_r3_aggregate_closure_r2.py
```

The runner refuses dirty source trees, creates a fresh per-run report directory,
executes every mandatory gate itself, writes machine-readable command result
records, generates per-case scenario results, builds the source handoff archive,
generates aggregate evidence, writes a closure manifest, and packages an
evidence archive.

The evidence generator:

```text
scripts/stage5d_final_restart_r3_aggregate_evidence_r2.py
```

is fail-closed. It rejects missing command records, non-zero exits, source-ref
drift, stale log hashes, missing per-case results, opened closed-surfaces and
scenario inventory drift. It does not synthesize default `true` proof fields.

## Mandatory gates

The aggregate r2 runner executes:

- aggregate checker self-test;
- six focused positive owning-test groups covering all 21 cases;
- package negative matrix;
- forged riskgate receipt negative;
- Stage 5C API freeze;
- Stage 5D additive freeze;
- forbidden-surface scan;
- no-Redis smoke;
- golden fixture drift check;
- Stage 5D negative harness;
- `cargo fmt --all --check`;
- `cargo test --workspace --all-targets`;
- `cargo test --workspace --doc`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- source/archive handoff safety.

Each command has a `*.result.json` record with command, source ref, timestamps,
exit code, stdout/stderr SHA-256 and toolchain.

## Not included

Stage 5D remains a no-I/O semantic foundation. It does not include production
durability backend, Redis integration, FINAM calls, command dispatch, runtime
live attachment, execution journal, broker reconciliation loop or live trading.

Those remain later-stage work.
