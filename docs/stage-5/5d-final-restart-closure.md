# Stage 5D final restart closure

Status: Stage 5D-final-restart-r2 retained foundation, no-I/O. The follow-up
Stage 5D-final-restart-r3 review remains HOLD / CHANGES REQUIRED; see
[5d-final-restart-r3-discovery.md](5d-final-restart-r3-discovery.md).

This slice closes the Stage 5D restart path from the actual
`HybridIntradayRuntimeStrategy` source state into a canonical persistence
envelope and durable riskgate ledger evidence package, then back through the
already accepted Stage 5D restore bridges.

The trading boundary remains closed:

- no Redis;
- no FINAM;
- no broker transport;
- no dispatch/send;
- no runtime-live;
- no broker execution.

## Added crate-private production surface

The new export coordinator is crate-private and source-owned:

- it reads semantic state from `Strategy::state(strategy)`;
- it reads runtime-private persistence data via the existing
  `stage5d_export_runtime_private_extension()` source bridge;
- it derives broker-neutral recovery indexes from the source semantic state and
  expected working-set hints;
- it binds account, instrument, strategy id, profile/config fingerprints,
  schema versions, lifecycle watermarks and riskgate persistence into one
  `Stage5dPersistenceEnvelope`;
- it computes the canonical payload checksum before the envelope can cross a
  restart boundary;
- it returns only a redacted evidence report with fingerprints and closed-surface
  booleans.

It does not serialize raw private runtime internals outside the Stage 5D schema,
does not repair a positive envelope after export, and does not open any broker
or transport surface.

Stage 5D-final-restart-r2 keeps the crate-private
`Stage5dCanonicalRestartPackage` boundary. The package carries:

- strict Stage 5D persistence envelope JSON;
- strict authoritative riskgate ledger evidence JSON;
- independent section checksums;
- package-level checksum over committed metadata and both durable sections;
- full-contract validation before strict package serialization;
- source/recovery fingerprints in the redacted export report.

Restart tests decode from the serialized package into a fresh runtime object
after dropping the source runtime. A dedicated poison test mutates the original
source object after export, drops it, and proves that fresh restore keeps the
pre-poison state from the package.

## Positive restart matrix

The focused final test matrix covers canonical source export, local durable
package JSON serialization, strict restart decode/checksum validation,
loaded-state binding, private apply, broker-truth bootstrap, riskgate injection,
runtime-state-restored callback and explicit Stage 5C warmup continuation for:

- clean flat state;
- open Long state;
- open Short state.

The existing accepted Stage 5D tests remain the parity oracle for:

- source-produced current-shadow Long/Short/realized-PnL;
- single-row and multi-row pending riskgate finalization recovery;
- non-empty known-order and pending-request retained indexes;
- restart-safe recovery-frontier execution to completion;
- explicit Stage 5C continuation after restored callback.

The r2 inventory also pins additional package families that must remain visible
before Stage 5D closure: pending entry/exit, deferred entry/exit, partial entry,
safe-mode close-only, known-order/pending-request indexes, working protective
order hints, already-complete recovery plan, and source-callback current-shadow
Long/Short/realized-PnL.

## Crash, golden and negative coverage

The r2 closure adds direct negatives for:

- post-export positive-envelope mutation;
- package metadata mutation;
- riskgate evidence section mutation even when package checksum is recomputed;
- source runtime poisoning after package export;
- removal of r2 full-matrix, source-callback, crash-store, negative-matrix,
  golden-vector, Stage 5C continuation and full-package-validation proofs.

It also adds deterministic durable-store crash/replay coverage for constructed
no-bytes, truncated write, full-bytes-without-commit, committed-before-ledger,
ledger-before-materialized, materialized-before-runtime-ack,
runtime-ack-before-final-checkpoint, restart-after-each-recovery-action,
idempotent replay-after-applied-action and multi-row between-row crash states.

Golden-vector coverage pins stable package bytes, checksums, semantic
fingerprints, recovery index fingerprints, recovery plan fingerprints, receipt
summaries and redacted reports for flat/open/pending/multi-row restart families.

The existing Stage 5D gates continue to cover the broader crash/negative matrix:

- malformed/truncated JSON and unknown fields;
- unsupported schemas, source build ids, config/profile/binding mismatches;
- semantic/private-state contradictions;
- noncanonical decimal and authority-generation mismatches;
- unexplained riskgate frontier lag;
- missing/duplicate outbox rows;
- acknowledged/materialized states without source-exact ledger evidence;
- broker-truth position/side/protective-order contradictions;
- non-paper/runtime-host/intent-sink boundary opening;
- callback attempts before recovery completion.

The machine-readable scenario inventory is:

```text
docs/stage-5/stage5d-final-restart-r1-scenario-inventory.json
docs/stage-5/stage5d-final-restart-r2-scenario-inventory.json
```

The additive freeze checker validates the r2 stage id, 51-row order, unique case
ids, closed-surface contract, owning tests, package sections and marker-pinned
negative cases.

## Review gates

Required gates for this candidate:

```text
cargo fmt --all --check
cargo test -p strategy-runtime-core stage5d_final -- --nocapture
cargo test -p strategy-runtime-core stage5d_b2bc1r3 --lib
cargo test -p strategy-runtime-core stage5d_b2bd1 --lib
python3 scripts/stage5c_api_freeze_check.py
python3 scripts/stage5d_additive_freeze_check.py
bash scripts/forbidden_surface_scan.sh
python3 scripts/stage5d_additive_freeze_negative_harness.py
bash scripts/stage5d_b2bc_review_gate.sh
```

## Next step

Do not move to Stage 5E from the r2 candidate. The next required work is the
Stage 5D-final-restart-r3 closure sequence, preceded by the documented
restore-shape freeze-extension if the MR pending-entry restore gap is accepted
as a source-runtime correction.
