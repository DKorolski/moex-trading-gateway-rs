# Stage 5G-e-b — owned canonical NewPackage application

Status: implementation review candidate.

Accepted predecessor: `0c1f1ce61c11c311e5df42edd4ed8c35beb838d2`.

## Boundary

The `Stage5gNewPackageCandidate` created by Stage 5G-e-a is consumed exactly
once by `apply_stage5g_new_package_candidate`. Its exact owned
`Stage5gCanonicalOrderPositionEvidence` is transferred to one crate-private
Stage 5G-c canonical application core. The public raw-evidence API performs the
only canonicalization pass and converges on that same core.

No raw evidence is cloned, decoded from diagnostics, reconstructed or
canonicalized a second time.

## Commit rule

Before application, only the pre-candidate checkpoint is persistable. A
successful `Awaiting`, normal `Converged` or R3 `MarketTerminal` result owns:

- the accepted Stage 5G-c capability;
- a checkpoint constructed from the accepted Stage 5G-c replay projection.

The complete applied replay projection must be structurally equal to the
classifier candidate projection, including package discriminator, current
identity, fingerprint ledger, total sequence, continuation watermark and
duplicate counter. A mismatch fails closed and returns only the pre-candidate
checkpoint.

A transactional Stage 5G-c block owns the exact returned session, typed reason,
canonical identity and pre-candidate checkpoint. It exposes no candidate
checkpoint; corrected BrokerTruth must be freshly classified from the old
commit.

## Executable coverage

- Awaiting commits exactly once after application;
- normal convergence preserves one Stage 5C callback;
- R3 Market-terminal convergence preserves one Stage 5C callback;
- incomplete BrokerTruth blocks transactionally without replay append;
- dropping a candidate leaves the old checkpoint reclassifiable;
- session/checkpoint mismatch blocks before application;
- compile-fail witnesses cover pre-apply checkpoint access, candidate reuse and
  blocked candidate checkpoint access.

## Deferred

Stage 5G-e is not closed. Canonical Stage 5D clean-process restore and the
GRST01–GRST12 restart/reconciliation matrix remain subsequent sub-stages.

Stage 5G-f, Redis live consumer/groups, FINAM transport, HTTP POST/DELETE,
broker execution, runtime-live, real orders, Stage 6, main merge and deployment
remain closed.
