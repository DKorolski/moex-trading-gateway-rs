# Stage 5G-e-a — replay commit type-state barrier

Status: implementation review candidate.

Accepted predecessor: `54e26c886afd97cd443fd8b0728fe180ff4793b5`.

## Scope

Stage 5G-e-a is the first deterministic restart/reconciliation slice. It closes
the persistence-authority gap carried forward by accepted Stage 5G-d without
yet restoring a runtime or opening Stage 5G-f.

`classify_stage5g_post_checkpoint_evidence` now returns an owning enum:

```text
Stage5gCheckpointReplayResult
  ExactReplay(Stage5gExactReplayCheckpoint)
  NewPackage(Box<Stage5gNewPackageCandidate>)
```

Only `Stage5gExactReplayCheckpoint` exposes `checkpoint()` and
`into_checkpoint()`. An exact replay mutates no broker state, so its updated
local sequence and duplicate counter may be persisted immediately.

`Stage5gNewPackageCandidate` owns:

- the exact pre-candidate committed checkpoint;
- the accepted Stage 5G-d candidate replay projection;
- the exact canonical broker-evidence candidate.

It deliberately exposes no candidate `checkpoint()` and implements no
`Serialize`, `Deserialize`, `Clone` or `Copy`. Its public diagnostic surface is
limited to the pre-candidate checkpoint and canonical evidence identity. The
crate-private owning extraction exists only for the next Stage 5G-e transition,
which must consume the candidate through the accepted Stage 5G-c authority.

## Persistence rule

```text
ExactReplay:
  committed checkpoint available immediately

NewPackage:
  pre-candidate committed checkpoint available
  candidate checkpoint unavailable
  future apply must consume the owned canonical candidate
```

No raw evidence is re-canonicalized and no candidate checkpoint can be
persisted through the public R5 replay API.

## Executable witnesses

- `stage5ge_a_exact_replay_alone_exposes_the_committed_checkpoint`;
- `stage5ge_a_new_package_retains_only_the_pre_candidate_committed_checkpoint`;
- compile-fail witness proving `Stage5gNewPackageCandidate::checkpoint()` does
  not exist;
- all accepted Stage 5G-d timer/replay tests remain green.

## Deferred Stage 5G-e work

The following remain required after this type-state barrier:

1. consume the owned candidate through the accepted Stage 5G-c transition;
2. expose a committed candidate checkpoint only after successful apply;
3. reuse the accepted Stage 5D canonical restart package for clean-process
   export/decode/private apply;
4. restore ACK/order/position/timer/escrow ownership;
5. run the complete twelve-case restart matrix and fresh mock BrokerTruth
   reconciliation.

## Closed surfaces

Stage 5G-f, Redis live consumer/groups, FINAM transport, HTTP POST/DELETE,
broker execution, runtime-live, real orders, Stage 6, main merge and deployment
remain closed.
