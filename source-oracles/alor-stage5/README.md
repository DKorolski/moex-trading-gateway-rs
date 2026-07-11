# ALOR Stage 5 integrated-wrapper source oracle

This directory contains the exact sanitized source selected as the Stage 5B-2
oracle for the integrated IMOEXF `HybridIntradayRuntimeStrategy` wrapper.

Selected source:

```text
repository: sanitized ALOR working repository
commit: 43242c89944d335d9cb0729b38bdd7d658378d5e
commit date: 2026-07-10T10:54:41+03:00
source path: strategy-runtime/src/strategies/hybrid_intraday_runtime.rs
source sha256: 6e15ab1b7212c56d3ecd8397b2d8991c1feccbde8eaa5e3d0051aec82a55f0aa
source lines: 6203
```

The previously reviewed 6,113-line source was the direct parent version:

```text
commit: f7525987dcf6196bf52d3b4dab866a3d3d2153ad
source sha256: 9704181eb72ffa2d4a9bb5d81f8902b46a4988e47ca45e0eb9f1af851e2c907a
source lines: 6113
```

The selected commit adds the later MR bracket terminal-reconciliation
hardening. Its wrapper diff against the parent is 93 insertions and 3
deletions. This explains the earlier source-oracle mismatch; the Stage 5 oracle
is intentionally the later `43242c8` source.

The source file is included only for provenance, review and future controlled
migration. It is not part of a Cargo crate and is not compiled or invoked.
