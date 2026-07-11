# Stage 5B-1a — correspondence, wrapper-oracle and profile hardening

Status: implementation review candidate.

Date: 2026-07-11.

## Purpose

This patch closes the P1 findings from the Stage 5B-1 review without changing
the imported production strategy logic.

It addresses three blockers:

1. the mutable correspondence ledger could be changed together with a formula;
2. the selected integrated-wrapper oracle was not reproducible in the handoff;
3. the configuration ledger mixed active high180 behavior with inactive classic
   MR configuration.

## Immutable Stage 5B-1 correspondence lock

`forbidden_surface_scan.sh` now embeds the accepted Stage 5B-1 manifest as
immutable scanner constants:

- schema version;
- stage id;
- exact ALOR source commit;
- exact seven source paths and source hashes;
- exact seven target paths and target hashes;
- exact change classes;
- closed production/transport/endpoint flags.

For `CopiedUnchanged` files the scanner requires:

```text
actual target SHA256 == ledger target SHA256 == frozen source SHA256
```

For `NamespaceOnly` `orchestrator.rs` it requires the accepted source/target
hash pair and the production-region SHA256 before `#[cfg(test)]`:

```text
ca836ded92cc7b9872482103f48dccac87b7b79d9ad9433979ee2069195dfb53
```

The scanner also requires the exact set of seven `.rs` files in the imported
`hybrid_intraday` directory. Stage 5B-2 must use a separate manifest rather than
editing the accepted Stage 5B-1 manifest.

The negative harness now proves that the scanner rejects:

- a production formula change even when `target_sha256` is recalculated in the
  adjacent ledger;
- replacement of the accepted ALOR source commit;
- source/target file drift;
- an added transport dependency;
- integrated-wrapper oracle drift;
- high180 profile artifact drift.

The negative harness first proves its copied baseline passes, so a negative
case cannot pass merely because the temporary test tree is incomplete.

## Reproducible integrated-wrapper oracle

The exact selected source is included at:

```text
source-oracles/alor-stage5/hybrid_intraday_runtime.rs
```

Selected provenance:

```text
commit: 43242c89944d335d9cb0729b38bdd7d658378d5e
sha256: 6e15ab1b7212c56d3ecd8397b2d8991c1feccbde8eaa5e3d0051aec82a55f0aa
lines: 6203
```

The previously reviewed source is the direct parent:

```text
commit: f7525987dcf6196bf52d3b4dab866a3d3d2153ad
sha256: 9704181eb72ffa2d4a9bb5d81f8902b46a4988e47ca45e0eb9f1af851e2c907a
lines: 6113
```

The newer selected commit adds MR bracket terminal-reconciliation hardening
(93 insertions, 3 deletions in the wrapper). The mismatch is therefore a known
source-lineage difference, not an unexplained Stage 5A typo.

The scanner locks the included wrapper oracle hash, line count, only-one-wrapper
file set and required high180 binding markers.

## Active high180 configuration freeze

The target binding is now explicit:

```text
profile = imoexf_primary_riskgate_high180_lb120
mr_variant = high180
active engine = High180MrEngine::new(High180MrConfig::default())
orchestrator path = on_bar_with_mr_override
```

Active high180 values:

- relative range: `0.005..0.050` exclusive;
- `k_long=0.085`, `k_short=0.090`;
- stop multiplier `7.0`;
- max hold `180 minutes`;
- entry cutoff `11:59:59`;
- target is current-day high/low midpoint;
- stop distance is seven times entry-to-midpoint distance.

Classic `MeanReversionConfig` values remain documented as present but inactive
for the target high180 entry path. `normal_append` remains ledger/shadow memory,
not an enforced entry gate.

Reproducible redacted artifacts:

```text
config/imoexf-hybrid-high180-profile.redacted.toml
sha256=15e31d7a285f1c8c80e9168a9098e37e56bbd60ab3ab3264592d23605708dfe4

tests/fixtures/stage5/imoexf_high180_profile_binding.json
sha256=ec6daea39f19f3162da5e8d77abb0f03a3f4f5ea2e2876c1d1e189401580ec5d
```

The new freeze tests verify:

- exact `High180MrConfig` defaults;
- integrated wrapper markers for the high180 override path;
- a classic MR candidate is not selected when a high180 override candidate is
  supplied to the orchestrator.

## Safety boundary

No runtime wrapper is compiled or attached by this patch. The included wrapper
is an uncompiled source oracle. The patch does not add a FINAM/Redis transport,
command consumer, POST/DELETE path, runtime-live readiness or stop/bracket
execution.

## Acceptance and next gate

After review acceptance:

- Stage 5A source/config freeze may be accepted;
- Stage 5B-1 source import may be accepted;
- Stage 5B-2 implementation may use only the included `6e15...` wrapper oracle;
- Stage 5B-2 must receive its own source correspondence manifest.
