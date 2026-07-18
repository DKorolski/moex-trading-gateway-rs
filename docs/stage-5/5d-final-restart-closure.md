# Stage 5D final restart closure

Status: implementation candidate, no-I/O.

This slice closes the Stage 5D restart path from the actual
`HybridIntradayRuntimeStrategy` source state into a canonical persistence
envelope and back through the already accepted Stage 5D restore bridges.

The trading boundary remains closed:

- no Redis;
- no FINAM;
- no broker transport;
- no dispatch/send;
- no runtime-live;
- no broker execution.

## Added production surface

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

## Positive restart matrix

The focused final test matrix now covers canonical source export, local durable
JSON serialization, strict restart decode/checksum validation, loaded-state
binding, private apply, broker-truth bootstrap, riskgate injection and
runtime-state-restored callback for:

- clean flat state;
- open Long state;
- open Short state.

The existing accepted Stage 5D tests remain the parity oracle for:

- source-produced current-shadow Long/Short/realized-PnL;
- single-row and multi-row pending riskgate finalization recovery;
- non-empty known-order and pending-request retained indexes;
- restart-safe recovery-frontier execution to completion;
- explicit Stage 5C continuation after restored callback.

## Crash and negative coverage

The final closure adds a direct negative for post-export positive-envelope
mutation: a semantic mutation after export but before restart is rejected by the
strict checksum boundary.

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

## Next step after acceptance

If this closure is accepted, Stage 5D can be treated as restart-capable for the
paper/no-send runtime path. The next roadmap item should be a separate Stage 5E
integration decision: how and when to attach the restored paper runtime to
gateway shadow data on VPS without enabling live execution.
