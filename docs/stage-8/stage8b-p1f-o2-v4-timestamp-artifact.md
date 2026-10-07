# O2 V4 timestamp successor — full binary review candidate

Date: 2026-10-05. NOT INSTALLABLE; O2 HOLD.

## Accepted source and artifact scope

[Independent source/authority acceptance](reviews/REVIEW_f3b3499_O2_V4_TIMESTAMP_RU.txt):
`f3b349949802abd5eff80cad6b9e9fc37bc327e1`, tree
`e9a744c59d43064283620903ea9b79175073efab`; its source implementation parent is
`c5545da055ec91035a9ee695dbe6c7c6ad2c67a9`.

All three binaries are compiled together from this exact accepted Git snapshot.
The packaging successor adds tooling, fixtures, review/status documents and
refreshes only the control inventory for updated status/roadmap documents.
Production inventory, crate Rust/Cargo, workflows, historical acceptance pins and
closed flags remain unchanged. No source semantics are reopened here.
Local WS/scheduler WIP is excluded through a separate clean worktree.
The additional Rust probe is tooling under `scripts/fixtures`, not a crate or
a feature compiled into the shipped binaries.

| ELF | SHA-256 |
| --- | --- |
| stage8b-p1f-o2-materializer | `f954bf98fa24c62dfa669eb159b7dc06717fe5edb61587c5404001ed38bdd0a3` |
| stage8b-p1f-o2-operator | `81e88b129896287e2b6b849043d2dfb09b860ddcd3861d1094f7e76a857b40e7` |
| stage8b-p1-paper-supervisor | `b70d1eaf222c747909a907de3af565f601c7a2d4f2c8caaed1b0851d8bcfba1b` |

Pinned image, default release feature selection and full build recipe are
inherited from the accepted sparse artifact. The first clean Linux/amd64 offline
build completed in 3m01s; the fresh build retained in the final package completed
in 3m05s and produced the same three ELF hashes. The existing default-feature publisher dead-code
warnings are retained, not suppressed. Containers run under Mac emulation,
not native VPS/systemd. No host credential, SSH agent, Docker socket, .env,
operational Redis or private ceremony is mounted; container network is none.

The first Linux CLI qualification attempt passed 2/4 tests and failed 2/4 with
`Io(ReadOnlyFilesystem)`: existing fixture helpers create `crate/target` scratch
directories. The runner now mounts two disposable tmpfs scratch directories at
those paths, keeping exported source read-only. No Rust/test assertion changes
or permission relaxation on source files. The initial failure log is retained
at [this path](evidence/o2-v4-initial-linux-cli-failure.txt); a fresh build/proof
directory is used for the corrected qualification, not an overwritten result.
The next attempt stopped before any test at OCI mount setup because tmpfs
mountpoints did not yet exist beneath the read-only bind. The runner now creates
only those two empty directories in its disposable exported build tree before
mounting. Git blobs stay unchanged; fixture writes remain tmpfs-only. Both
environment failures are retained, and final qualification starts in a new
proof directory. No failed run is relabelled PASS.

## Qualification boundary

The standalone probe links exact default-feature release rlibs from that build:
404 real History M10, including 18 sparse buckets, plus a labelled synthetic
seven-M1 candidate. Capture is `04:10:03.123456789Z`; the receipt is 1 ns earlier.
Production assembly creates V4; the production parser and protected staged
consumer validate the same exact bytes. Guardian summary remains seconds-only.
A rehashed source with capture truncated to the second rejects both in the full
parser and in staged consumption. No tolerance, clock sleep or receipt rewrite.

The inherited exact-ELF smoke checks bootstrap schema 2, policy V3, no-riskgate
identity/fingerprint and mixed-config rejection. Materializer stops at the
deliberately wrong synthetic account boundary before token/network access.
Operator runs the inherited custody/no-selector smoke. This is not a claim of
a successful FINAM network collection or full operational O2 process.

Additional existing tests are executed under Linux/amd64, release mode, against
the accepted exported source, without feature expansion:

- 2 assembly/diagnostic tests;
- 4 fixed materializer/staged-consumer tests;
- 8 V4 parser/legacy/freshness/first-boot/recovery tests;
- 1 guardian materialization/pending recovery/exact replay test (multiple internal frontiers).

The guardian fixture is separate from the standalone release-library probe;
this is composition coverage of the production paths, not one end-to-end
operational process. No new fixture API or production guardian change is added.
The package verifier requires all 15 tests, their named selection, exit codes,
log hashes and unchanged ELF hashes before/after qualification. It pins all
deterministic probe outputs; generated hashes alone cannot relabel the witness.

## Reproduce and audit

```sh
python3 scripts/stage8b_p1f_o2_v4_timestamp_build.py NEW_BUILD --registry-cache PUBLIC_CARGO_REGISTRY
python3 scripts/stage8b_p1f_o2_v4_timestamp_artifact.py qualify NEW_PROOF --build NEW_BUILD
python3 scripts/stage8b_p1f_o2_v4_timestamp_artifact.py package NEW.zip --build NEW_BUILD --proof NEW_PROOF
python3 scripts/stage8b_p1f_o2_v4_timestamp_artifact.py check NEW.zip
python3 scripts/stage8b_p1f_o2_v4_timestamp_gate.py NEW.zip NEW_GATE_DIR
```

Historical sparse tooling remains byte-for-byte unchanged. The timestamp
entrypoint explicitly binds the existing packager/checker to accepted source
and review pins and adds fractional qualification checks; it takes no arbitrary
source override. Full project files, hidden .github, fixtures, both Git trees,
ELFs, build/qualification logs and current acceptance review enter the ZIP.
`artifact-v4-timestamp/descriptor.json` and `handoff-commit.txt` distinguish
compiled accepted ref from packaging review ref. Neither is deployment authority.

The postseal local gate binds the archive SHA and reviewed source inventory,
checks current-tree authority and its 45 negatives, strict workspace Clippy,
fmt, unchanged production/workflows and 22 archive tamper controls. Results
are retained in `.local-gate.zip`; no fresh GitHub CI or full workspace Rust
test run is claimed. Existing accepted source-test evidence remains historical.

## Next boundary

Independent artifact acceptance and ordinary CI precede a new installation
package/evidence. Any later installation must preserve actual terminal history
and receipts, last recorded **FAILED / generation 1 / sequence 8**. Do not reset
genesis, replay a terminal phase, mix fractional writers with old V4 consumers,
or replace just one ELF. A fresh calendar/window must be bound at installation;
the October 2 qualification fixture is not an operational calendar.

Installation and a single bounded O2 each require separate permission. The
timestamp defect is proven, but its relation to the October 5 VPS failure is
not proven. No VPS inspection/mutation, installation, scheduling or phase
execution occurs in this slice. FINAM POST/DELETE, broker dispatch, real orders
and runtime-live stay closed. WS/continuity/freshness/EOD and later O3/O4 parity
remain in the existing roadmap, not inferred from these offline checks.
