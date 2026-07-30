# Stage 5F authority-rotation protocol

## Purpose

The protected-base `Stage 5F Base Authority / base-authority` check rejects
ordinary modification of its authority roots. This document defines the single
in-band path for a reviewed transition to the next Stage 5F delivery slice. It
does not open Rust, Redis, FINAM, dispatch, runtime-live or broker execution.

The workflow runs `stage5f_base_authority_contract.py` from the protected PR
base. The pull-request head, including this manifest, is input data only and
is never imported or executed by that check.

## Rotation PR contents

The PR adds or replaces
`docs/stage-5/stage5f-authority-rotation.json` with exactly this JSON shape:

```json
{
  "authority_files": {
    "<each current authority path>": "<candidate SHA-256>"
  },
  "canonical_ci_gate_sha256": "<candidate SHA-256>",
  "changed_paths": {
    "<every changed source path except this manifest>": "<candidate SHA-256>"
  },
  "kind": "stage5f-authority-rotation",
  "next_generation": 2,
  "next_stage": "5F-b-fixture-input-redacted-fingerprint-schema",
  "previous_base_sha": "<full protected PR base SHA>",
  "previous_generation": 1,
  "previous_state_sha256": "<base authority-state SHA-256>",
  "schema_version": 1
}
```

`changed_paths` intentionally excludes the manifest itself: placing its own
digest in that map would create an unverifiable self-hash cycle. The contract
calculates every other source-tree change and requires an exact match. It
rejects deletion and all special files.

The candidate `stage5f-authority-state.json` must be the exact successor of
the base state: one generation higher, the same `next_stage`, and the supplied
base SHA and state digest. `authority_files` must provide the exact candidate
digest of every authority file, including files whose bytes remain unchanged.

The following values must be equal, not merely present:

```text
manifest canonical_ci_gate_sha256
canonical CI embedded Stage 5F gate digest
inventory gate digest
entry-checker static gate digest
handoff-checker static gate digest
actual gate SHA-256
```

## Scope and approval

The manifest may bind only the existing authority files, Stage 5F scripts,
Stage 5F docs and fixtures, `README.md`, `docs/current-status.md` and
`docs/handoff.md`. It cannot bind a `crates/`, broker, transport or operational
path.

The contract establishes technical eligibility; it does not replace human
authorization. Before merging a rotation PR, GitHub protection must require a
unique `Stage 5F Base Authority / base-authority` check, one independent review
after the latest push, an up-to-date branch and no direct/force/admin bypass.
After merge, a disposable clean PR and an adversarial authority-drift PR must
show the new authority accepts the first and rejects the second before the next
Stage 5F implementation slice starts.
