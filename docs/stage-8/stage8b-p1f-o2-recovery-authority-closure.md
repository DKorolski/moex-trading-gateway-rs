# O2 recovery — accepted-source authority successor

Date: 2026-09-28. Status: SOURCE ACCEPTED / AUTHORITY REBOUND;
fresh required PR checks and merge are pending. No operational permission.

## Immutable source acceptance

- Accepted source: `5b8f878833dbf71fa7614152a4a5fb03cbf40b65`.
- Tree: `2339cdd897c1ca1fdabfd911b4768a7ea4062cfb`.
- Review baseline: `38c7b825863d8a54c018eb5581ed8e243e017cfe`.
- Source ZIP: `moex-trading-project-5b8f878-o2-recovery-review.zip`.
- ZIP SHA-256: `abc60b09022f35d6a02492e04a6503645405fc6caaf3b46241a5b0fdbe9a1ef7`.
- Independent review: `FINAM_5b8f878_O2_RECOVERY_SOURCE_REVIEW_2026-09-28.md`.
- Review SHA-256: `4759f46e8b878bd9a86ee13de46d968ca52e4c4e827501524ef081563b59d177`.
- Verdict: SOURCE ACCEPT; both concrete recovery findings closed.

This successor updates only the current-tree inventory and status documentation.
Rust/Cargo, CI workflows/checkers, deploy/config, strategy semantics and trust keys
are byte-for-byte unchanged from the accepted source. Production inventory changes
relative to the old authority are exactly the three accepted O2 files. Control
inventory refresh covers only current-status and roadmap bytes. Historical replay
refs, hashes, closed-surface flags and CI policy are unchanged.

The accepted source ZIP retains the full source gate logs, raw commit and tree
binding, ten new regressions and explicit evidence limits. Reopening filesystem
frontiers is not SIGKILL or Linux multi-UID execution. No new Rust execution is
claimed by this documentation-only successor. Its exact-commit authority check
and negative harness must pass locally; fresh GitHub `rust` and `redis-smoke`
checks must pass before the ordinary history-preserving PR merge. The old green
baseline CI result is not substituted for either gate.

## Next development slice: corrected O2 artifact

Use the exact synchronized merge commit, not the old artifact at `1090de4`.
Do not add a general recovery framework or repeat accepted historical reviews.

1. Rebuild the Linux/amd64 materializer and operator with the pinned build image
   and locked Cargo dependencies; retain raw build logs and exact source identity.
2. Regenerate binary/source/profile/unit identities and templates for accepted
   `imoexf-baseline07-bo-only-paper-v1`. Update the existing artifact validation
   and negative cases together; never reuse stale binary hashes or High180 profile
   assertions from the superseded artifact.
3. Reuse the existing GET-only, materializer, guardian and isolated bootstrap
   witnesses. Add focused exact-ELF/unit custody and process-exit evidence as
   required by artifact acceptance. The current operator process exit on runner
   failure is 70; typed StopNotProven code 72 is not the CLI exit contract.
4. Deliver a commit-bound artifact ZIP, checksums, safety report and bounded
   evidence for independent artifact review. Only then consider the separately
   authorized non-activating installation and one bounded O2 bootstrap.

Private seeds/tokens/account IDs, signed time-bounded authorizations and fresh
broker responses stay outside Git and handoffs. No VPS contact, service start,
operational Redis activation, FINAM write/dispatch, runtime-live or real orders
is performed or permitted by this closure. O3/O4 remain separate later slices.
