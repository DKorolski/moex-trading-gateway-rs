# October-7 O2 terminal-only materialization abort

Status: SOURCE / EVIDENCE REVIEW CANDIDATE. O2 HOLD.
Accepted predecessor: `d866cf992a963fbd76c4532aae14088f6e6046ee`.
Implementation follows `REVIEW_d866cf9_SOURCE_AND_TERMINAL_RECOVERY_20261007_RU.txt`.
The accepted fchmod prevention remains unchanged. No remote operations, service
changes, signing, deployment, authority rebind or Git push/merge were performed.

## Exact boundary

One fixed operator command, `terminal-abort-oct7-fixed`, extends the existing
guardian. It accepts no arguments, path overrides, replacement pins or clock.
Production pins bind phase `992be63a…`, predecessor event `7880c9da…`,
pending `42322f61…`, source `f4c135d7…` / 1,876,641 bytes, staged envelope
`7135486f…`, old installation `395f9e3e…`, generation 1, sequence 12,
deadline `2026-10-07T04:25:00Z`, and the accepted Ed25519 public key.
No private signing key is read or required.

Fresh preflight reads all 16 installed slots and exact old transaction trees,
the durable state skeleton, predecessor authority files, P0 unit properties
and fragments, and all four stopped P1 unit/cgroup/job proofs. The inventory
is a compiled, SHA-pinned public baseline extracted from the accepted October-7
setup, not a caller-supplied PASS assertion. It reports actual before/after
hashes and metadata. The staged package remains root:root 0400 in a 0700 root,
distinct from source-temp root:service 0400 in the 0750 config root.
No Redis query is necessary for this terminal-only operation.

The execution lock precedes the guardian lock. Guardian verifies the signed
phase, exact claim and predecessor chain, original pending/source identity,
absence of Ready/final source/config/owner, and all expected restart slots.
The original bad mode is evidence for this abort only. Normal materialization
and existing-file validation still require 0440.

## Transaction and history

1. Create-once/fsynced `materialization-abort-intent.json` in the exact manifest
   directory. It records original bytes/hash/custody/inodes, retained parent
   identities, fixed archive names and one terminal decision timestamp.
2. Archive original pending as `aborted-materialization-pending.json` (0440)
   and source as `aborted-materialization-source.json` (0400). No wrapping of
   the source into the 128-KiB authority JSON limit. Bounded no-follow reads,
   exclusive descriptor-relative copies, fsync, exact reread and directory
   sync precede freeing each original. This works across filesystem boundaries.
3. Consume only the verified original files under the existing locks. Keep the
   original inode open through unlink. A partial/foreign archive is an error,
   never automatically deleted, overwritten, chmod-repaired or guessed.
4. Reuse the terminal receipt -> event -> head ordering. The only new event is
   PHASE_TERMINAL / EXPIRED / 1 / 12. There is no Ready/O2_MATERIALIZED or extra
   terminal sequence. Existing event-temp and head-temp replay remain exact.
5. Persist completion, reread full history and immutable archives, then return.
   The terminal reason contains SHA-256 of a standalone canonical abort binding;
   history validation verifies the receipt, binding and archived evidence.

Incomplete intent (including a prepared intent) blocks normal inspect, admission,
claim and materialization. Only the exact private abort continuation can validate
its expected intermediate frontier. There is no general ignore-pending mode.
Lost-response replay retains receipt/event hashes, timestamp and sequence.
Completed history can admit a future newly signed phase; no old source temp
needs manual deletion. Such a claim is tested only, not issued operationally.

## Evidence and tests

Linked fixtures use the existing production V4 materialization path and a real
filesystem: five previous completed phases, ACTIVE/1/11, pending12, source0400.
Reopen tests exercise intent prepared/durable; first/full archives; released
fixed paths; terminal pending/receipt/event-temp/event/head-temp/head; pending
removal; completion before response. They are controlled durable faults, not
OS SIGKILL witnesses. Partial archive creation is separately fail-closed.

Negative cases cover phase/predecessor/pending/source/installation/signature,
generation/sequence/deadline, custody/links, published files/Ready/event/owner,
foreign intent/archive, substituted parent, and conflicting head temp.
Snapshots check file bytes, inode, owner and mode unchanged on initial rejection.
Completed archive corruption blocks ordinary inspection.
Fresh inventory helpers have real-file drift/hardlink/symlink tests; stopped
proof tests reject PID/control PID/job/cgroup/activity/foreign-unit mismatches.

The abort code has no FINAM/Redis client, Hybrid invocation or bootstrap launcher.
The existing I1 direct-effect audit asserts zero provider/callback/publication/
claim/XACK/schedule effects throughout each abort fixture, including failures.
Tests also check that it produces no Ready/owner/final config/source and only one
terminal event. The later fixture claim is separate from the abort assertions.
No operational effect counters or live execution evidence are claimed here.

Targeted gate: guardian debug/release, systemd tests, doctests, strict
affected-crate all-targets/all-features Clippy, fmt, diff, exact authority-drift
accounting and immutable source-tree packaging. Native Linux/root:service
custody qualification belongs to the next normal artifact gate, not this Mac
source review. Current-tree authority remains ACCEPTED_BASELINE_REBIND_PENDING,
not PASS or merge/deployment readiness.

From the clean source commit:

```sh
python3 -B scripts/stage8b_p1f_o2_abort_review.py gate tmp/o2-terminal-abort-review
python3 -B scripts/stage8b_p1f_o2_abort_review.py package tmp/o2-terminal-abort-review
```

## Next gates

Source acceptance -> one final authority rebind/native artifact qualification
-> separately authorized root-only staged operator against unchanged installed
`e05b4bf` manifest and three ELF slots -> actual EXPIRED/1/12 evidence review ->
one normal successor installation reusing the qualified artifact and preserving
history -> separately authorized fresh bounded O2.

Never install the successor first: the old phase/pending bind the old manifest.
Do not rearm the expired timer, retry materialization, refresh old timestamps,
extend deadlines or introduce a new recovery service. O3/O4 WS, continuity,
freshness/EOD and paper-vs-ALOR sessions remain ahead; real orders remain closed.
