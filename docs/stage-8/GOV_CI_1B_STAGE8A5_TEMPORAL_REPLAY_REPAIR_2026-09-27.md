# GOV-CI-1B Stage 8A-5 temporal replay repair

Status: governance-only review candidate. No production Rust, Cargo, workflow,
deployment material or operational configuration is changed.

## Failure reproduced

The canonical GitHub `rust` job for accepted source/governance commit `8f771d7`
failed inside the immutable Stage 8A-5 replay. Its detached Stage 7B checkout at
`a1044e0dbe324c722b637498ca80ffafd9f0cbee` contains an operational restart
fixture with `persisted_at = 1790000000` (`2026-09-21T14:13:20Z`). A source
callback stamps `Utc::now()`. Once wall time passed that fixture boundary, two
historical tests correctly rejected their generated envelope with
`TimestampChronologyInvalid`.

This is a time-expired historical test fixture, not a failure in the accepted
ALOR/FINAM parity correction, O2 code, strategy behavior or current production
tree.

## Narrow repair

The Stage 8A-5 gate itself remains byte-identical at SHA-256
`1361ad49d41351484cf61c86822deb640818e755b7b35bda44592fd437ff69f8`.
Only the 10 exact cargo-test checkouts in the inherited Stage 6/7/8A-5
gate graph that contain the same source pre-image receive the exact test-only
chronology repair already introduced and accepted in the current tree by
`e7ae487f9897be297bd9fabcee9ffad302e6dd3e`:

- source path: `crates/strategy-runtime-core/src/stage5d_persistence.rs`;
- original source SHA-256: `90ab3f9253c0b96fee8ea2c2aeb5e0eb9b0e4c99a3dfb0c404648b588c205eb2`;
- normalized source SHA-256: `a8caa83eacd4337c8562d24560c18cdd23d1f48f23115b5ec4c7b7561d38b6be`;
- exact diff SHA-256: `4b1f165f785ff72336002d8bfa90bd8535f9e6087a8769347afd72cc2ea07a83`;
- one replacement in `#[cfg(test)]`: persisted boundary `1790000000` becomes
  the accepted chronology ceiling `4102444800`.

The helper fails closed for any ref, pre-image, post-image, path, replacement
count, dirty-file inventory or diff mismatch. It emits deterministic evidence,
which the outer gate rereads after the nested replay completes.
The final evidence must enumerate exactly the 10 full refs pinned in
`gov-ci-1-authority.json`: `10e3578`, `2b6371a`, `2b6d6e9`, `8418cfb`,
`8d4c1f4`, `a1044e0`, `bf58b47`, `e0bf9b7`, `e10d8fb` and `ec71791`;
missing or additional refs fail closed. Deeper graph nodes are invoked with
their detached gates disabled, while design-only `00cead2` runs no cargo tests;
none of those non-executed nodes is normalized.
The cargo wrapper deliberately leaves `cargo fmt` and the historical Stage 7B
checker/negative inventory on the pristine commit. It applies the normalization
only to `cargo test` commands carrying both `--workspace` and `--all-targets`,
where the time-sensitive tests are actually selected. Every wrapped test then
restores the exact original pre-image, proves a clean worktree and preserves the
real cargo exit status before any historical preseal runs.

## Scope and transition

The current-tree production manifest remains byte-identical to accepted
`8f771d7`. Redis activation, O2 execution, FINAM POST/DELETE, broker dispatch,
runtime-live and real orders remain closed. After independent acceptance, the
repair commit may update PR 9; both required checks must pass before merge.
