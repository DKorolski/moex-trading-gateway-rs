# IMOEXF retained REST readback: preserve receipt precision

Parent: accepted source `bef44be9c01a5fa1ca12ef1d0b8af9f1220f004a`.
The observed IMOEXF trace was produced by `1b0104f9`, not by this new revision.
This is a narrow source correction; no deployment, restart or result acceptance.

## Observed failure

The 19 November bounded run recorded 20 canonical/model inputs and 40 paired
outcomes, then retained `cohort_verification_refused`. Its first rejected control
record was source snapshot sequence10. The original receipt was
`2026-11-19T04:10:01.063287124Z`; the reader manufactured the earlier as-of
`2026-11-19T04:10:01.063000000Z` by converting through `timestamp_millis()`.
The unchanged broker admission correctly rejected `received_at > trusted_now`.
All27 original GET snapshots passed admission with their exact acquisition
DateTime and original snapshot SHA, but failed with the reader's millisecond floor.

## Correction and unchanged boundaries

`restart::journal::admit_rest` now chooses the maximum **original DateTime**
and passes that exact acquisition time into retained-source admission. Its outer
`now: i64` future guard still compares at the existing millisecond resolution.
There is no added tolerance, freshness extension, modified raw body, rewritten
timestamp/hash, or claim that historical evidence is fresh/current DATA.

No numerical kernels, strategy/feedback, input source selection, WS/reconnect,
account/auth, production fingerprint rules, STOP deadlines, Cargo or CI change.
The separate USD/RI known-Bar-field, second-calendar-boundary and historical
calendar-coverage findings remain open; this correction does not resolve them.

## Regressions

Eight tests cover integer-ms positive, nonzero sub-ms roundtrip, multipart
receipts within one millisecond, later retained readback, the existing future
guard, wrong SHA/status/incomplete transport/reversed receipt, precise multipart
ordering, and all27 unchanged original IMOEXF receipts with their recorded hashes.
The public GET fixture is historical evidence only: no account ID, credential,
operational grant or model continuation capability.

## Fingerprints and retrospective evidence

`journal.rs` is part of the bridge implementation fingerprint. A newly built
reader must not present itself as the original `1b0104f9` implementation or
rewrite that failed generation's admission, marker, journal or SQL provenance.

The retained trace can be inspected in an explicitly **diagnostic** replay of
the pinned historical contract, with only this precision correction in source
admission. The historical implementation pin is confined to the external
cfg(test) diagnostic harness, never introduced into production compatibility
checks. That replay is not `verify_directory` acceptance: the original incomplete
marker remains, and no new completion is written into the old output directory.

Current-identity model/cohort fixtures and matching-reader regressions remain
the source gate for the corrected revision. Source acceptance must precede a
separate exact Linux release and separately authorized operational attempt.
No clean/full-session or ALOR-parity claim is made here.
