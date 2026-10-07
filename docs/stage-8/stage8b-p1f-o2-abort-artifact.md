# O2 October-7 abort: authority and full Linux artifact

Status: ARTIFACT / AUTHORITY REVIEW CANDIDATE, not operational authority.
Source accepted at `3bfd98ab6623c8b339eb56f4d9e28834640e1044` in the
[independent review](reviews/REVIEW_3bfd98a_O2_TERMINAL_ABORT_20261007_RU.txt).
This preparation changes no Rust/Cargo, compiled preservation fixture,
strategy, workflow, installed files or units. Current-tree authority is rebound
to the accepted production inventory and current documentation only.

## Qualification and handoff

`scripts/stage8b_p1f_o2_abort_artifact.py` exports the clean, direct preparation
successor of the accepted source, reuses the pinned offline Rust builder, and
packages all three default-feature release ELF binaries:

- `stage8b-p1f-o2-materializer`
- `stage8b-p1f-o2-operator`
- `stage8b-p1-paper-supervisor`

Build inputs are committed Git blobs, with a copied public Cargo registry only.
No checkout secrets, Docker socket, SSH agent or host operational paths enter
the containers. Networking is disabled. Linux/amd64 runs under Docker Desktop
on an arm64 Mac: real Linux syscalls and UID/GID custody, emulated x86 CPU;
not a bare-metal VPS execution claim.

Qualification retains the existing fractional V4, sparse/no-riskgate release
probe and exact-ELF smoke plus 15 inherited release tests. The accepted abort
fixtures run unchanged as UID 0 / GID 987 under umask0077: archive, consume,
15 reopen frontiers, 28 nonmutating negatives and partial-archive controls.
The existing ignored multi-UID probe is explicitly run, with service UID65534 /
GID987 permission checks. An unprivileged invocation of the delivered operator
must fail at its root-identity check before reading operational paths.
There is no production signing key, real incident source or remote command.

Run build, qualify and package from the same clean commit. Each stage refuses
foreign source/evidence. The handoff contains source/raw commit/tree manifest,
accepted source ZIP and review, build logs/recipe, three hashed ELFs, Linux
logs and authority checks. The checker reconstructs the tree and proves the
accepted production files did not change. Fresh GitHub CI remains separate;
no push/merge is performed by these scripts.

## Root-only staged recovery plan — NOT an execution approval

After artifact review/required CI and explicit operational authorization:

1. Retain a fresh read-only snapshot. Expected incident is ACTIVE/1/11 with
   pending12 and the accepted source0400. Four P1 units must be stopped and P0
   unchanged. Any drift stops the operation; do not edit the compiled baseline.
2. Place only the qualified operator in a new root-owned 0700 staging directory,
   outside `/usr/local/libexec/moex` and the installed transaction roots. Verify
   its exact package SHA-256, root ownership, 0700 mode, regular single link and
   parent custody. Do not place keys, copy credentials or replace installed ELF.
3. Keep the old installed manifest SHA-256
   `395f9e3e987ce6f5b2c52b6083d1ebc79c71287df72096c6d313ee3e56da32f5`
   and all three old binaries unchanged. The recovery command is exactly
   `terminal-abort-oct7-fixed`, with no arguments or overrides. Its internal
   fresh full preflight, execution/guardian locks and exact incident binding
   are mandatory; a recorded old PASS is insufficient.
4. Capture stdout/stderr/exit and post-state without modifying evidence. Expected
   result: one EXPIRED/1/12 event/receipt, archived original bytes/modes,
   source-temp and pending consumed only after durable archival, no Ready,
   bootstrap or owner, P0/P1 and all preservation slots unchanged. Exact replay
   after lost response must retain the receipt/event/time/sequence. A partial
   archive or nonzero result is a stop, not permission for chmod/rm or blind retry.
5. Submit operational terminal evidence. Only after acceptance prepare one
   normal successor installation using this same full artifact and retaining
   history. Fresh calendar/phase bindings and a new bounded O2 need separate
   authorization. Do not rearm the expired timer or refresh old source timestamps.

No remote contact or operational activation belongs to this package. O2 HOLD;
O3/O4 WS/continuity/freshness/EOD and ALOR paper comparison remain ahead.
