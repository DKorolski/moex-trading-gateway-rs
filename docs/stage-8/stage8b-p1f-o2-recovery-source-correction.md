# P1F O2 — retained replay and selector recovery correction

Status: SOURCE ACCEPTED at `5b8f878833dbf71fa7614152a4a5fb03cbf40b65`.
Baseline: `38c7b825863d8a54c018eb5581ed8e243e017cfe`.
Independent review: `FINAM_5b8f878_O2_RECOVERY_SOURCE_REVIEW_2026-09-28.md`,
SHA-256 `4759f46e8b878bd9a86ee13de46d968ca52e4c4e827501524ef081563b59d177`.
The [authority successor](stage8b-p1f-o2-recovery-authority-closure.md) records
the next transition; SOURCE ACCEPT does not itself authorize merge or execution.
This is the two-finding correction for PR #9, not an O2 activation package.
The baseline CI jobs `rust` and `redis-smoke` passed in run `36380295678`.
That CI result does not certify this subsequent source change.

## Scope and findings

Only three production files change: the O2 materializer CLI, operator CLI and
existing guardian. Cargo, workflows, strategy semantics, deployment units,
credentials, trust keys and accepted runtime profile remain unchanged.

- PR discussion `4116484549`: retained source has account alias
  `finam-paper-primary`, but fresh-admission replay previously supplied the raw
  credential account ID. Even an exact retained package failed identity validation.
- PR discussion `4116484551`: a new signed claim was committed before a selector
  writer that rejected every previous manifest hash. A valid replacement phase
  could therefore be stranded behind the old selector.

## Implementation

Retained materialization still checks the credential account hash against both
policy and evidence. The existing source parser now receives the independently
validated policy alias, as it does for initial materialization. Exact source and
package hashes, manifest binding and the 300-second broker-truth freshness boundary
are preserved. Retained replay performs no fresh GET and creates no new package.

The operator delegates selector publication to the guardian after `claim_phase`.
The new fixed-path method takes the existing guardian lease and revalidates
quarantine/history/current Active identity, O2 phase and deadline. It exposes no
caller-selected path. A delayed old publisher cannot overwrite a newer selector;
publication cannot race terminal history mutation under the same lease.

An existing selector must be either exact current bytes or a terminal O2 ancestor
in the validated history. Checking ancestors also covers an intermediate claim
that failed before publishing its selector. Unknown hashes, malformed bytes,
symlinks, hardlinks and wrong custody remain rejected. The selector is only a
projection; it neither issues nor changes signed authority.

Publication uses the existing durable create helper with a manifest-scoped
prepared filename, then atomic rename, directory fsync and exact reread. The helper
sets exact mode independently of umask and validates UID/GID. Retry after a durable
prepared file resumes that file; retry after rename syncs the directory again.
Old manifest-scoped prepared files may remain after a superseded interrupted
phase; they contain only a public manifest hash and are not selected or consumed
as authority. This patch does not introduce automatic forensic-file pruning.

Errors during custody checks or incomplete/conflicting temp writes remain
fail-closed; no arbitrary temp bytes are repaired or accepted. Recovery tests
cover complete durable prepared files, not every possible partial syscall write.

## Regression evidence

Four materializer tests cover exact repeated replay, account/manifest/hash/alias
conflicts, age 300 versus 301 seconds and raw-account substitution even with
recomputed hashes. They call the same retained-byte validator as the CLI with a
real first-boot source fixture and the production source parser.

Six guardian tests cover initial/idempotent publication; each terminal-state
replacement and reopening after claim-before-publication; durable prepared file
and rename-response-loss retries; an intermediate unpublished failed claim;
foreign selector/temp, symlink, hardlink and mode rejection; competing guardian
lease, wrong phase and expired claim rejection. Authority sequence/history stay
unchanged during projection publication.

These are real local filesystem transactions and reopen tests at simulated
durable crash frontiers, not a new SIGKILL or Linux multi-UID claim. Operational
paths are never used by the tests. No VPS, FINAM or operational Redis is contacted.
Inherited isolated Redis regression tests may start their own local test servers.

Reproduce this historical source gate from a clean checkout of the accepted
source commit `5b8f878` (it deliberately expects the pre-rebind authority):

```sh
python3 scripts/stage8b_p1f_o2_recovery_review.py gate tmp/o2-recovery-UNIQUE
python3 scripts/stage8b_p1f_o2_recovery_review.py package tmp/o2-recovery-UNIQUE
python3 scripts/stage8b_p1f_o2_recovery_review.py check reports/handoff/ARCHIVE.zip
```

The gate records affected-package debug tests, release materializer/guardian
tests, accepted ALOR source-correction regressions, doctests, workspace strict
all-feature Clippy and format/diff checks. ZIP includes raw logs, exact source
manifest, raw Git commit object and reconstructed Git-tree verification, plus
external SHA-256 and safety JSON. It is a source package, not a Linux build.

FINAM debug/release/doctests use the accepted CI default feature selection;
`--all-features` belongs to Clippy, not these tests. An exploratory all-feature
FINAM test run failed the unchanged legacy
`endpoint_gate_marker_cannot_be_forged_from_manual_decision`: it expects the
`m3j16-actual-one-shot` feature to be disabled. No test is skipped or altered to
hide that failure. The durable package's all-features tests enable only its
existing fixture features. The original failed log is retained locally under
`tmp/o2-recovery-review-20260928-r1/finam-debug.txt`.

## Merge and operational boundary

At source-review time the production authority remained pinned to the previous baseline. Its
current-tree checker is expected to reject exactly the three changed production
entries; the evidence records that rejection explicitly, not as authority PASS.
No authority/checker/workflow files are changed by this source patch.

Source review is now accepted. Next: the normal narrow authority
rebind for the accepted source and fresh PR checks, followed by history-preserving
merge. Rebuild O2 from the exact merged source with reviewed profile/unit identity.
O2 artifact review, non-activating installation and operational permission remain
separate. No service start, operational Redis activation, FINAM write/dispatch,
runtime-live or real orders is authorized here. No historical re-audit is needed.
