# Stage 5D-b1 — dual-baseline enforcement migration

Status: review candidate. Scope: enforcement-first implementation.

Stage 5D-b1 implements the additive freeze enforcement layer required before
any persistence DTO mutation or runtime-private restore logic.

## 1. Implemented

- Stage 5D additive manifest:
  `docs/stage-5/stage-5d-additive-freeze-manifest.json`.
- Stage 5D additive checker:
  `scripts/stage5d_additive_freeze_check.py`.
- Stage 5D negative harness:
  `scripts/stage5d_additive_freeze_negative_harness.py`.
- Self-validating manifest contract:
  checker-owned constants validate manifest checker path, negative harness path,
  closed surfaces, negative case list, Stage5d public symbols, current
  compatibility checker, and historical closure checker artifact.
- Historical Stage 5C checker artifact:
  `tests/fixtures/stage5/stage5c_api_freeze_check.closure.py`.
- Stage 5C checker compatibility mode:
  Stage 5C public API shape remains frozen, while approved Stage 5D bridge
  files move to the additive baseline.
- Forbidden surface scanner migration:
  scanner now runs Stage 5C API freeze and Stage 5D additive freeze, while the
  Stage 5D negative harness is a separate explicit gate.
- Additive bridge regions:
  `lib.rs`, `hybrid_intraday_runtime.rs`, and `stage5c_paper_host.rs` have
  explicit `STAGE5D-ADDITIVE-BRIDGE` markers.
- Stage 5D public skeleton:
  `stage5d_persistence.rs` exposes only opaque `Stage5d*` names and no runtime
  mutation.

## 2. Dual-baseline contract

The Stage 5C closure baseline remains immutable:

```text
commit: 69cc73b7f33d8cb418c784ac993856d8a487693d
archive: moex-trading-project-69cc73b.zip
archive_sha256: 0b614ebe83b0a8af85cde0ca7a1ae481457813edad72626cd4bb5972c9c83f91
manifest_sha256: f8c555d11de1271f5041b4d3abf880ac7a406d6fb23f5e4d38ca25468a974323
report_sha256: 1d15c992ce1658fea6d7ec8a25094b094400ba00b764ac23d32c525207d19b48
original_checker_sha256: e494e92ffb5f8d90b6a581c7b99e4e80f1906aeedfa1e7446d428eb31c757209
```

The Stage 5D additive baseline pins current bridge files and verifies that
removing approved additive regions reproduces the Stage 5C closure source
hashes.

## 3. Negative coverage

The negative harness requires failures for:

- Stage 5C public API drift;
- trading-region drift;
- additive-region escape;
- public non-Stage5d namespace leakage;
- raw strategy extractor;
- missing historical Stage 5C baseline;
- closed-surface downgrade;
- negative-case removal;
- manifest checker / negative harness path drift;
- Stage5d public symbol removal/addition;
- current compatibility checker drift;
- historical checker missing/content drift/substitution;
- legacy Stage 5C persisted-restore production bypass via direct call, alias,
  multiline call, function reference, and qualified path with whitespace.

The normal forbidden-surface scanner does not run the full negative harness
inline, to keep the normal scanner bounded. Handoff/CI must run the negative
harness as a separate gate.

## 4. Explicitly not implemented

Stage 5D-b1 does not implement:

- persistence envelope DTO mutation;
- runtime-private snapshot export/apply;
- Stage5c/Stage5d transition functions;
- riskgate injection;
- Redis bridge;
- FINAM execution;
- broker transport;
- runtime-live;
- broker order execution.

## 5. Next slice

Only after Stage 5D-b1 acceptance:

```text
Stage 5D-b2 — versioned persistence envelope DTO and exact private-state bridge.
```
