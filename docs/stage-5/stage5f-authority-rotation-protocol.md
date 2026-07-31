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
    "<each current authority path>": {
      "git_mode": "100644",
      "sha256": "<candidate SHA-256>"
    }
  },
  "canonical_ci_gate_sha256": "<candidate SHA-256>",
  "changed_paths": {
    "<every changed source path except this manifest>": {
      "git_mode": "100644",
      "sha256": "<candidate SHA-256>"
    }
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
calculates every other source-tree change from the committed Git tree and
requires an exact `{git_mode, sha256}` binding. It rejects deletion, gitlinks,
symlinks and every non-blob or unsupported mode; `100644` and `100755` are the
only permitted Git modes.

The candidate `stage5f-authority-state.json` must be the exact successor of
the base state: one generation higher, the same `next_stage`, and the supplied
base SHA and state digest. `authority_files` must provide the exact candidate
Git-mode and content binding of every authority file, including files whose
bytes remain unchanged.

`schema_version`, `authority_generation`, `previous_generation` and
`next_generation` are exact JSON integer fields. The protected-base contract
checks their type before arithmetic or equality comparison: `true`, `1.0` and
`3.0` are invalid. This prevents an apparently accepted rotation from leaving a
non-integer state that later rotations cannot consume.

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
path. The canonical `ci.yml` is immutable even in a rotation. A rotation may
change only the exact base-authority workflow
`.github/workflows/stage5f-base-authority.yml`; an ordinary PR may not alter
any `.github/workflows/**` path. This prevents a second workflow from
manufacturing an ambiguous `base-authority` required-check namespace.

The contract establishes technical eligibility; it does not replace human
authorization. Before merging a rotation PR, GitHub protection must require a
unique `Stage 5F Base Authority / base-authority` check, one independent review
after the latest push, an up-to-date branch and no direct/force/admin bypass.
After merge, a disposable clean PR and an adversarial authority-drift PR must
show the new authority accepts the first and rejects the second before the next
Stage 5F implementation slice starts.

## Bounded activation-repair sequence: r8 → r8a → r9

The first external activation exposed a platform dependency in the existing
forbidden-surface scanner: the hosted runner did not provide `rg`. This is not
an authorization to alter canonical CI. The only permitted recovery is the
three-generation sequence below.

Generation 2, `5F-a-r8-bootstrap-repair-authority`, changes only the
protected-base contract and its governance evidence. It does not modify the
scanner or a workflow. The subsequently discovered Stage 5D freeze binding of
the R9 worker/harness requires one more governance-only step: generation 3,
`5F-a-r8a-stage5d-freeze-rebind-authority`. R8a changes only the protected-base
contract and governance evidence; it does not modify either Stage 5D artifact.

Its successor contract then admits exactly generation 4,
`5F-a-r9-portable-forbidden-scanner`, only when the protected base has schema
version 1, authority generation 3 and the exact R8a stage name. The candidate
must declare the matching `previous_generation = 3`, `next_generation = 4`
and R9 stage. It may then alter only these source paths:

```text
docs/current-status.md
docs/handoff.md
docs/stage-5/5f-a-atomic-hybrid-semantics-entry.md
docs/stage-5/5f-a-r8-bootstrap-repair-authority.md
docs/stage-5/stage-5d-additive-freeze-manifest.json
docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json
docs/stage-5/stage5f-authority-rotation-protocol.md
docs/stage-5/stage5f-authority-rotation.json
docs/stage-5/stage5f-authority-state.json
scripts/forbidden_surface_negative_case_worker.sh
scripts/forbidden_surface_negative_harness.py
scripts/forbidden_surface_scan.sh
scripts/handoff_safety_check.py
scripts/stage5f_atomic_hybrid_semantics_entry_check.py
scripts/stage5f_base_authority_negative_harness.py
scripts/stage5d_additive_freeze_check.py
```

The r9 candidate must change `scripts/forbidden_surface_scan.sh` as executable
mode `100755`; omitting that change, changing its mode, adding an arbitrary
path, or using this special stage to change a workflow is rejected by the
generation-3 contract. Generic rotations cannot change any of the three
forbidden-surface scanner/harness files or the two paired Stage 5D rebind
artifacts. Replaying R9 from generation 4 or a later generation, invoking it
from a different generation-3 stage, or spoofing the R8a stage name on a later
state is rejected. The exception is therefore consumed by R9 and does not
create a general scanner-maintenance route.

R9 replaces the six scanner text searches with a Python 3.11+ implementation
and uses a portable worker-side infrastructure-error search. Before the
candidate is sent to hosted CI, the full 87-case negative harness must pass
with `rg` absent from `PATH`. That evidence confirms the repair condition while
leaving canonical CI byte-for-byte unchanged. Hosted `rust` and `redis-smoke`,
then the clean and adversarial PR checks, remain the activation gate.
