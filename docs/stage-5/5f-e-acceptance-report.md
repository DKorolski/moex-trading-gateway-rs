# Stage 5F-e aggregate acceptance

Status: aggregate review candidate
Accepted predecessor: `1a41b530419d39ddc84fff81a9dfdde6ede878ce`
Target: IMOEXF Hybrid, canonical final M10, paper-only, no send

## Accepted Stage 5F-d input

Independent review accepted the complete Stage 5F-d atomic Hybrid matrix at
commit `1a41b530419d39ddc84fff81a9dfdde6ede878ce`. The reviewed source archive was
`moex-trading-project-1a41b53.zip` with SHA-256
`18d7944264ade10ea2f0860b861a7176ba98fe5d82c9beaf1cbcd22b72e5b2b3`.
The acceptance record supplied for Stage 5F-e has SHA-256
`3ffeb72698a472f7857b2b430ead81560c886fb77f6a4d3a64e501253b271eec`.

Stage 5F-e does not rewrite the accepted Stage 5F-d candidate artifacts. It
freezes their hashes in `stage5f-final-scenario-inventory.json` and records the
independent acceptance as a separate governance fact. This keeps the important
distinction:

```text
accepted Stage 5F semantic evidence
!= production/live strategy golden authorization
```

No scenario, formula, strategy parameter, fingerprint algorithm, callback
route or expected result is changed by this package.

## Aggregate closure claim

The aggregate package must prove all of the following from one commit-bound
source tree:

- all 34 Stage 5F rows and all 16 official groups remain present;
- the disposition split remains 26 accepted transitions, one structural
  invariant, three blockers before callback and four terminal outcomes after
  callback;
- exact ordered intent vectors, pre/post fingerprints, request order,
  callback/observer/settlement cardinality and settlement identity remain
  frozen;
- the sole Stage 5C/B3F route remains the only accepted semantic route;
- three independent focused matrix executions produce one identical
  fingerprint-vector SHA-256 and one identical semantic-evidence SHA-256;
- debug, release and default-parallel focused runs pass;
- inherited B1, R3 and B3F authority gates pass from their immutable snapshots;
- B3F detached provenance remains 580/580 and the production UI remains 8/8;
- Stage 5D negative coverage remains at least the accepted 303 cases;
- the portable forbidden-surface negative matrix remains 87/87 with `rg`
  absent from `PATH`;
- workspace fmt, tests, doctests and clippy pass;
- Redis regression smoke passes against an isolated disposable Redis process;
- source-tree and ZIP provenance are independently reconstructible.

## Scanner authority reconciliation

The old Stage 5F-a/B1 forbidden scanner is intentionally not widened for the
later test-only Stage 5F-d module. Its positive proof is executed from the
immutable accepted B1 snapshot. The portable 87-case negative harness is also
executed with `rg` absent. Current-tree Stage 5F-d and Stage 5F-e checkers then
own the later additive surfaces. This is the already accepted authority split,
not a waiver and not a weakening of the scanner.

## Generated closure artifacts

The commit-bound handoff builder emits:

```text
reports/stage5f/stage5f-acceptance-result.json
reports/stage5f/stage5f-fingerprint-reproducibility.json
reports/stage5f/stage5f-negative-result.json
reports/handoff/moex-trading-project-<commit>.zip
reports/handoff/moex-trading-project-<commit>.zip.sha256
```

The ZIP additionally contains a full source-tree manifest, the raw Git commit
object, one result record plus stdout/stderr evidence per required gate, a
top-level evidence manifest, a handoff marker and the final archive-safety
record. Generated evidence is not committed back into the source tree.

## Closed surfaces

Stage 5F-e remains evidence/governance-only. It does not open:

- Redis command consumption;
- FINAM transport or HTTP POST/DELETE;
- dispatch or broker execution;
- runtime-live or real orders;
- ACK/order/trade/position/timer/restart feedback;
- stop, SLTP, bracket or protective-order lifecycle.

Those lifecycle capabilities remain Stage 5G work. Stage 5G is not authorized
by this review candidate; it may start only after an independent review accepts
the complete Stage 5F-e handoff.
