# Stage 5B-1b — structural source-freeze hardening

Status: repeat-review candidate.

Date: 2026-07-11.

## Purpose

This patch closes structural bypasses around the accepted Stage 5B-1 semantic
kernel. It does not migrate, compile, export, or attach the integrated runtime
wrapper.

## Locked build graph

The forbidden-surface scanner now requires all of the following:

- a Python 3.11+ stdlib `tomllib` parser reads both Cargo manifests; regex is
  not used to infer workspace or target configuration;
- `crates/strategy-runtime-core` is a parsed explicit workspace member and is
  not workspace-excluded;
- the crate manifest has the accepted SHA256
  `00f18c0d3ddc6f7fb4196edc2a51f18da034070555aad980c35098cbd4ed5fd0`;
- package integration tests remain enabled and no custom build target exists;
- the library target remains `src/lib.rs` with tests enabled;
- `src/lib.rs` has the accepted SHA256
  `eba13a333fc0c003d9afa96f379cfb833b3148d549b97425406f4386bc3cea4a`;
- the complete production Rust source set under `src/**/*.rs` is exactly
  `src/lib.rs` plus the seven files in the accepted source-correspondence
  ledger;
- the complete crate Rust target set is exactly those production files plus
  the two accepted integration tests, whose content hashes are also locked;
- default `build.rs`, extra integration targets, binaries, examples and benches
  are absent and fail the exact target-set check.

The manifest and library-root hashes make target redirection or export changes
fail closed. The complete source-set check prevents an untracked wrapper or
alternate production module from being introduced beside the accepted kernel.

## Negative harness

The negative harness proves scanner rejection for:

1. removing `strategy-runtime-core` from workspace members;
2. commenting out the real member while leaving its quoted text in a TOML
   comment;
3. redirecting `[lib] path` to `src/alternate.rs`;
4. adding and exporting an untracked production wrapper;
5. disabling package integration tests with `autotests = false`;
6. adding an auto-discovered default `build.rs`;
7. adding an untracked integration-test wrapper target;
8. adding untracked bench or example wrapper targets.

It also proves that the bracket terminal-reconciliation fixture cannot drift
without scanner failure.

## Boundary

This hardening changes only evidence and structural enforcement. It adds no
runtime host, command consumer, FINAM send path, Stop/SLTP/bracket endpoint, or
continuous-live capability. Stage 5B-2 implementation remains blocked pending
review of this structural freeze and its wrapper semantic inventory.

## README history

The concise README rewrite was committed separately as `ea8bca0`. Commit
`1ea8fde` only added two Stage 5 documentation links. The README refactor is
therefore not part of the structural hardening implementation diff.
