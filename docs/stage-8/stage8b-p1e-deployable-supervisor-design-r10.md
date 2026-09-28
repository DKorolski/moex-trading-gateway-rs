# Stage 8B-P1-e R10 — outcome/source and I0 evidence closure

Status: design/checker review candidate. Direct parent: `4051a7b4d2c810100aaf983bddede62dbd03d96f`. Accepted production predecessor remains P1-d4 closure `c2a9e1246dfdd59f3a6297268de907dedcb19903`.

R10 is the narrow correction requested by the independent R9 review. It does not change production Rust, Cargo, workflows, configuration, deployment, Redis, FINAM, broker dispatch, runtime-live, or real-order behavior. R9 reattachment/seal separation, E05/E25 precedence, source/timer ordering, both latch checkpoints, and the integrity/semantic-negative split remain authoritative.

## Exact counter model

The V1 fixture set remains the exhaustive set of 46 authenticated outcomes. R10 composes it with `stage8b-p1e-route-outcome-counter-contract-v2.json`.

The measurement window starts after consumption of the selected route-bound continuation permit and ends inclusively at the returned outcome boundary. `replacement_seal_commit_total` counts only successful calls to `commit_stage8b_p1_replacement_seal` in this window. It does not count journal transactions, logical continuations, a preexisting authenticated seal, or read-only validation.

The corrected source-derived cases are:

- FX10 / LR10: exactly one post-permit P1-d4 publication revalidation and zero replacement seals;
- LR12 candidate semantic zero/one: reconstructed transition seal plus semantic outcome seal, total two;
- LR12 candidate semantic multi: reconstructed transition seal only, total one;
- LR15 semantic zero/one: one semantic outcome seal;
- LR15 semantic multi: zero new seals; existing authenticated S_eval or S_terminal remains restart authority;
- LR12 recovered cancel: reconstructed transition seal plus recovered-cancel seal, total two.

The accepted-source oracle binds exact hashes of `redis.rs` and `recovery.rs`, exact entrypoints, ordering, fixture bindings, and source scan obligations. Production source is not changed to fit the oracle.

## I0 acceptance provenance

I0 is run only from a clean committed direct descendant of accepted R10. Cleanliness includes untracked files. HEAD and tree are captured before execution and must be identical after all tests. A dirty developer run cannot receive acceptance PASS.

The entrypoint requires a fresh absolute retained-output path outside the repository. It creates a sibling temporary directory and atomically renames it into the requested output. Both PASS and FAIL executions are retained. The retained set contains the complete gate log, run result, source-tree manifest, artifact manifest and digest, crash evidence, and targeted test list/run logs. A later handoff must either bundle this directory or bind it by exact digest and source ref/tree.

The gate, scope checker, retained-evidence helper, and reusable P1-d4 checker are SHA-256 pinned by the V3 contract. They are excluded from the I0 helper allowlist; changing one requires a separate reviewed correction.

## Executed exact regressions

Each of the six P1-d2/P1-d3 tests uses its full libtest module-qualified name. The gate first invokes `--list --exact` and requires exactly one selected matching name. It then runs `--exact` and requires exactly one passed test. Zero-test success, a renamed test, or a missing test fails closed.

The remaining I0 obligations are unchanged: reusable P1-d4 content validation and source negatives, complete crate suites, 105 crash cells in two clean runs, retained-evidence validation and negatives, both doctest suites, formatting, and strict clippy.

## Authorization boundary

Independent acceptance of R10 authorizes only I0 production seam implementation in the existing three-file allowlist:

1. `crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs`
2. `crates/runtime-durable-service/src/stage8b_p1_semantic.rs`
3. `crates/runtime-durable-service/src/lib.rs`

It does not authorize supervisor implementation, service installation, operational DB0/VPS activation, FINAM POST/DELETE, broker dispatch, runtime-live, or real orders.
