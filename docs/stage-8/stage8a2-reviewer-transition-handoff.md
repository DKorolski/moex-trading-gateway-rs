# Reviewer transition handoff — Stage 8A-2

## Review starting point

Repository: `https://github.com/DKorolski/moex-trading-gateway-rs`

Working branch: `stage8a2-builder-composition`

Accepted predecessor branch: `stage8a1-protected-capability`

Accepted predecessor commit:
`1ff04154ba4b7a5ee060a73b853ce89bd7442f44`

Accepted predecessor archive: `moex-trading-project-1ff0415.zip`

Archive SHA-256:
`02a332af7ccf1815a79de0f9a025be9d31094160f7977cd3e34de270497138a0`

Independent final review SHA-256:
`ac11ffae08cc11c6e11f031deec9ca4e6d55b76eae6bcc5887f85b0fb913c74c`

## Project position

- Stages 0–5: broker-neutral contracts, ALOR parity extraction, strategy
  semantics and deterministic paper/restart behavior are closed.
- Stage 6: durable request/client/broker identity and crash recovery are closed
  at `10e357825a701193d964975bb5769bd0745d4986`.
- Stage 7A: paper command-consumer contract is closed at
  `2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64`.
- Stage 7B: production-grade durability composition is closed; final accepted
  aggregate source is `a1044e0dbe324c722b637498ca80ffafd9f0cbee`.
- Transition Gate 7→8 is closed at
  `630bef3fb9aa07bbc377498fc052f085155a043c`.
- Stage 8A-0 is closed at
  `c949d7f83aa87cf990204a5b8ae66e5ca37c9f1d`.
- Stage 8A-1 is closed at
  `1ff04154ba4b7a5ee060a73b853ce89bd7442f44`.
- Stage 8A-2 is the only open slice.

The accepted Stage 8A-1 source must be treated as an immutable predecessor,
not as a fresh design proposal. A finding that would reopen it should identify
a concrete P0/P1 regression or a newly demonstrated exploit against the exact
accepted artifact.

## Stage 8A-2 review target

Stage 8A-2 composes only the existing vetted FINAM PLACE/CANCEL builders behind
a mock/no-send boundary. It consumes a freshly revalidated opaque Stage 8A-1
continuation, proves MARKET/LIMIT/CANCEL shape parity and emits only an opaque
internal witness plus redacted diagnostics.

It does not classify HTTP outcomes, reconcile broker truth or send a request.
Those remain separately reviewed Stage 8A-3/8A-4 concerns.

The normative entry scope is
[stage8a2-entry-contract.md](stage8a2-entry-contract.md). The stable sequence is
[stage8-slice-plan.md](stage8-slice-plan.md).

## Carry-forward from the outgoing reviewer

1. New Stage 8A-2 builder/no-send rules need semantic negative mutations; exact
   file-hash drift alone is not enough.
2. Stage 8A-1 does not prohibit all external calls to its owner-mediated
   constructor. It prohibits unauthenticated raw-path construction; possession
   of the recovery owner, lifecycle key and accepted config digest is the
   trusted composition authority.

These are Stage 8A-2 review considerations, not blockers against the accepted
Stage 8A-1 artifact.

## Known tooling boundary

The repository-wide `scripts/forbidden_surface_scan.sh` still embeds a
historical Stage 5 workspace/topology baseline. On the current accepted tree it
reports already accepted Stage 6/7 crates as drift and is not a valid Stage 8
acceptance oracle. The Stage 8A-1 closed-surface checker also embeds the exact
historical R3 changed-path allowlist and should be reproduced at the accepted
`1ff0415` source rather than run as a successor-branch scope checker.

The transition archive instead verifies all pinned Stage 8A-1 source and
contract hashes with `git_scope=false`, requires the successor diff to equal
the exact docs/tooling transition allowlist, and contains no production Rust
change. Stage 8A-2 must add its own stage-scoped closed-surface scanner and
semantic negative harness before implementation acceptance. This limitation is
recorded explicitly; no PASS is claimed for the stale repository-wide scanner.

## Surfaces that remain closed

- FINAM POST/DELETE and any non-loopback broker order endpoint;
- automatic retries and HTTP outcome classification;
- broker reconciliation and constructible no-match authority;
- Redis live command consumption and broker dispatch;
- runtime-live and real strategy orders;
- STOP/SLTP/bracket/replace/multi-leg execution;
- Stage 8B and all later live authority.

## Suggested review order

1. Verify predecessor commit/archive/review hashes.
2. Confirm the Stage 8A-2 changed-path set contains no predecessor mutation.
3. Review the capability-to-builder composition and exact durable identity.
4. Run golden, compile-fail and semantic negative suites.
5. Run closed-surface and archive-safety gates.
6. Decide only Stage 8A-2; do not implicitly open Stage 8A-3 or Stage 8B.
