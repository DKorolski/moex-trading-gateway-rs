# Sparse M10 — source acceptance and narrow authority closure

Date: 2026-10-03. Source ACCEPTED; closure qualification/review pending.

Accepted source: `d63c378a51899a4d407dddc732896e89ccc51b43`.
Tree: `aff81bbb27b0247ea603ef97d868edd6246590ac`.
Parent: `eb1a974d884196f9620f3db2cced3a4ad0991921`.
Source ZIP SHA-256:
`30b1dccaf53f3e82fe29b062c83dec13c2605e9de212a47609499a9c4e16ff0d`.
The [independent review](reviews/REVIEW_d63c378_SPARSE_M10_RU.txt) accepts
bounded observed-M1 admission, source V4, first boot and continuation, not an
installed continuous daemon. The source ZIP and its original authority failure
are immutable historical evidence; a new authority PASS does not rewrite them.

## This delta

Refresh only production/control inventories in `gov-ci-1-authority.json` plus
these status/review documents. Keep checker, workflow, accepted replay pins,
requirements, closed flags and legacy examples unchanged. All Rust and Cargo
bytes remain identical to the accepted source. This is not a second source
implementation or a CI redesign. Local checker and negative results are new
closure evidence; the accepted source's Rust logs are inherited, not rerun claims.

## Next artifact

Build three Linux/amd64 release ELFs from an exact pinned commit with accepted
production bytes. Explicit artifact-only fixtures must bind materialization
policy V3, bootstrap schema 2, source plan/wire V4 and the no-riskgate profile.
Do not relabel the accepted strict artifact or mutate legacy examples. Keep
build/source and packaging identities separate and reconstruct both Git trees.
The artifact and its ordinary CI require their own results. A local result
does not assert a GitHub check or independent artifact acceptance.

## Operational boundary

O2 HOLD. No VPS, operational Redis, FINAM, secrets, installed files, service
start, automatic root migration or main merge in this slice. FAILED generation
1 / sequence 6 and all preceding terminal receipts/history must survive the
later, separately accepted installation. A fresh identity does not reset history.
One bounded O2 requires separate authorization after artifact/installation gates.
Continuous sparse input, WS readiness/reconnect, freshness/duplicates/overlap
and EOD remain in the existing operational checklist for O3/O4. No new framework.
