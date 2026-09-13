# Stage 8B-P1-e I1 transaction V5 implementation

Status: source review candidate. Accepted predecessor:
`21eaf01916f2da5eaacb191b4d7339a8101070ad`.

This slice implements the crash-safe filesystem boundary authorised by the
independent I1 governance-closure review. It does not claim complete deployable
I1 acceptance.

## Implemented boundary

- the accepted first-boot source now enters one transaction V5 function;
- marker V4 is HMAC-authenticated and advanced through exact generations 1–5;
- the initial Stage 7 journal and recovery seal are split by a linear
  journal-durable capability so the marker can be persisted at the real
  durability frontier;
- the initial Stage 6 envelope is schema V2 and binds immutable first-boot
  provenance to the operational identity, runtime profile/config, source
  bundle, source plan, history, riskgate observations and candidate semantic
  identity;
- receipt V2 is written with create-exclusive temp, `sync_all`, no-replace
  rename, parent fsync, authenticated reread and exact adoption cross-check;
- ordinary run becomes eligible only after the canonical marker reaches
  `adopted` and both authority temp paths are absent;
- all V5 predicates are evaluated without precedence and zero/multiple matches
  fail closed;
- all ten implemented write-frontier hooks classify deterministically after a
  simulated abrupt stop;
- the four post-seal response-loss states recover only through an exact
  transaction-id/action selector and converge to the same adopted authority;
- Stage 6 checkpoint advance and Stage 5G replacement preserve byte-identical
  V2 provenance. Legacy V1 packages retain their accepted schema and are never
  implicitly upgraded; the strict V2 replacement API rejects V1.

## Deliberately deferred

Pre-seal administrative actions (`remove-marker-temp`, `resume-prepared`,
`quarantine-root`, `finalize-quarantine` and the three pre-adoption marker-temp
completions) remain classified and fail closed, but are not mutation-capable in
this slice. They belong to the separately reviewed bootstrap-recover command
boundary. The deployable owner loop and the process signal/panic/restart matrix
also remain later I1 slices.

Operational Redis DB0/DB15, VPS activation, operational credentials, FINAM
POST/DELETE, broker dispatch, runtime-live and real orders remain closed.

## Review evidence

The stage gate runs formatting, source-contract mutations, focused V2 and
transaction tests, both complete affected crate suites, doctests and strict
clippy. The inherited 105-cell P1-d4 matrix is rerun serially with the accepted
larger test-thread stack because concurrent execution can exhaust the default
libtest stack on macOS.

Independent acceptance of this source slice may authorise the narrow
bootstrap-recover administrative continuation. It must not be interpreted as
permission to activate Redis or FINAM execution.
