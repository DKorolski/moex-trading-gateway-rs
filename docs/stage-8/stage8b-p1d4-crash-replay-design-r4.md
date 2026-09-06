# Stage 8B-P1-d4 crash/replay design R4 correction

Status: R4 design-only review candidate. P1-d4 source commit is paused until
independent R4 acceptance.

Accepted design predecessor:
`e1ce6d3baec3974d8dfd05c2f3de00110e0605bf` (P1-d4 R3 design,
CLOSED / ACCEPTED).

Accepted business predecessor:
`7dc7c802feca6e79d3a1a9902c181ad7b6afc506` (P1-d3 governance closure,
CLOSED / ACCEPTED).

R4 is a narrow implementation-discovery correction. It changes exactly three
proof cells and four fields. It does not change the 88 general requirements,
92-cell inventory, scenario/frontier set, accepted P1-d3 semantics, normal
business path, persistence schema or closed surfaces.

## Normative artifacts

The unchanged general acceptance matrix is:

```text
stage8b-p1d4-crash-replay-acceptance-matrix.csv
rows: 88
sha256: e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de
```

The historical R3 registry remains immutable:

```text
stage8b-p1d4-scenario-frontier-matrix-v3.csv
cells: 92
sha256: 8c3122af016e860e3fa54a6f4143c50b9258c2a5a4bd2d15f9847a118a8582cc
```

The sole active R4 proof-cell registry is:

```text
stage8b-p1d4-scenario-frontier-matrix-v4.csv
cells: 92
cell IDs: P1D4C-001..P1D4C-092
scenarios: S01..S11
frontiers: F00..F20
```

Its SHA-256 is bound by the R4 checker and evidence after generation.

## Correction C1: durable-equivalence restart classification

P1D4C-064 S09/F09 and P1D4C-092 S09/F04 have the same authenticated durable
suffix after SIGKILL: recovered CANCEL V3 plus RequestFinalized, without
`S_cancel_recovered`. An ACK that existed only in memory cannot participate in
restart classification.

The exact correction is:

```text
P1D4C-064.expected_restart_disposition
  P1d3CancelContinuationPending -> P1d3PreAckPending
```

No other P1D4C-064 field changes. Its sole continuation remains exact recovered
ACK replay followed by `S_cancel_recovered`. P1D4C-091 and P1D4C-092 remain
`P1d3PreAckPending`.

`P1d3CancelContinuationPending` remains a required and distinct durable
equivalence class at P1D4C-063 S09/F08, where target `S_terminal` is present
but recovered CANCEL V3 and RequestFinalized are absent.

Forbidden alternatives:

- reading a crash marker or test environment from production recovery;
- adding a durable ACK-applied marker only to distinguish F04 from F09;
- preserving volatile ACK state across SIGKILL;
- collapsing the target-terminal continuation into final completion.

## Correction C2: independent later-bar and generated-command sources

The accepted F15 rule is authoritative: after same-bar callback and exact
one-intent publication are durable, only the exact originating bar XACK
remains. The generated command lifecycle belongs to its own command M10 source
and must not be synchronously settled under the replaced P1-d3 bar package.

The exact P1D4C-039 correction is:

```text
only_legal_continuation:
  confirm_exact_existing_publication_then_exact_bar_xack_generated_command_lifecycle_independent
```

The exact P1D4C-040 corrections are:

```text
expected_restart_disposition:
  P1d3TruthCommitted -> P1SemanticPrepublicationReady

only_legal_continuation:
  prove_bar_pel_absent_and_group_frontier_then_continue_exact_existing_command_without_callback_replay
```

All other P1D4C-039/P1D4C-040 fields remain unchanged. In particular:

- callback replay delta remains zero after F15;
- provider delta remains zero at the bar-source frontier;
- schedule authority reissue remains forbidden;
- F16 requires parsed Redis XACK reply `1` plus absent exact PEL and an
  authenticated group frontier before `AlreadyAcknowledged` is credited;
- the exact existing generated command continues through the accepted P1-d2
  lifecycle without a second callback or publication.

Forbidden alternatives:

- retaining Stage5C settlement authority in a P1-d3 replacement package;
- introducing a combined P1-d3-plus-Market schema or owner in P1-d4;
- completing the generated command before bar XACK;
- replaying the callback or publishing a second command after F15/F16;
- losing the independently durable generated command when the bar is XACKed.

## Exact delta contract

Compared with v3, v4 must differ in exactly these four `(cell, field)` pairs:

```text
P1D4C-039 only_legal_continuation
P1D4C-040 expected_restart_disposition
P1D4C-040 only_legal_continuation
P1D4C-064 expected_restart_disposition
```

Every other byte-level CSV field value is inherited from v3. Row count,
ordering, IDs, duplicate/conflict obligations and all 21 frontiers are
unchanged.

## Source resumption after R4 acceptance

After independent acceptance, the existing source worktree may be completed
only by:

1. removing the attempted volatile/durable-union distinction for S09/F09;
2. recovering F04 and F09 from their shared durable suffix as
   `P1d3PreAckPending`;
3. XACKing the exact later-bar source after durable callback/publication;
4. recovering the independently published Market command as the accepted
   P1-d2 prepublication owner;
5. running all 92 positive cells and every duplicate/conflict variant;
6. proving the exact marker, SIGKILL, PEL, XACK-last and evidence contracts.

Source implementation acceptance remains separate from this design review.
P1-e remains unauthorized until corrected P1-d4 source acceptance and a
separate governance-only current-tree authority rebind.

## Closed surfaces

R4 keeps all of the following closed:

```text
operational Redis DB0/VPS
paper supervisor / P1-e
FINAM POST/DELETE
broker dispatch
runtime-live
real orders
partial fills
fees/slippage
replace/protective/bracket/multi-leg orders
Generation-2 production authorization
```
