# USD/RI source component — narrow R1 correction

R0 review: SOURCE CHANGES REQUIRED on
`3c18bc8d9e5e77f1e3da76c72df682432929da3f`. This annex supersedes only the
two defective checks in the R0 source-component contract. No new operating mode.

## SRC-1 — frozen numerical identity

Both reference lot and contract size are compared exactly as `Decimal` against
the accepted catalog: USDRUBF = 1000; RIZ6 = 1. Equivalent decimal spellings are
accepted without float conversion. Wrong positive values (including 777) fail.
The public validated identity retains both original size strings and its reader
checks both against the same frozen values. `trade_lot_size=1` remains separate;
reference size never becomes order quantity. Tick, currency, account, expiry and
no-rollover checks stay intact. No numerical kernel, profile or fee change.

## SRC-2 — reference evidence is semantic, not a hash-shape flag

Each native `add_reference` call retains its actual `recorded_at_ms` along with
the full `ClosedM1SnapshotEvidenceV1` and admitted snapshot hash. At attachment,
the complete snapshot is revalidated using the existing guarded REST admission.
Holding an older native capability does not freeze freshness indefinitely.

The same check is used in the offline reader at the retained event time:

- correct instrument and prior physical-subscription record;
- reference within its independently admitted calendar's bootstrap start and
  current session cutoff;
- ordered event clock after subscription and before the bounded run deadline;
- planned closed request ranges, provider depth, complete ordered HTTP parts;
- original request/receive clocks, status, transport completeness, raw body
  bytes/hash/length, mapped instrument, M1 identity and values;
- exact equality with the admitted snapshot hash, not just 64 hex characters.

Missing/malformed/mutated evidence refuses COMPONENT_PREFIX_VERIFIED even if
all control indexes, predecessor hashes and the whole control chain are resealed.
The reader validates original times; it never substitutes inspection-time clocks.
No canonical publication capability or physical WS owner is created from files.

## Evidence

The two inspected reviewer Rust tests were executed against a copy of R0 with
unchanged production: both fail at their intended assertions. With R1 both pass.
Four additional tests cover equivalent-size positive controls, full native
references for both instruments, 16 resealed reference mutations, and stale
native references refused before logging/SQL. The four wrong-positive size
variants are exercised by the reviewer's nested instrument/field assertions.

The retained positive reference scenario runs actual native `add_reference`
twice, then two native M10 commits and four outcomes, followed by read-only
SQL/control inspection. It is synthetic source evidence, NOT FINAM acquisition.
The original separate four-arm fixture remains 26 inputs / 52 outcomes /
6 states / 4 ready arms / 8 virtual marks. Its numerical model state/marks are
checked against R0, excluding deliberately changed source provenance/result IDs.

Raw logs include R0 expected failures, R1 reviewer PASS, matching production
CLI refusals for every mutant, and precise offline-reader error reasons. Full
broker/signal-shadow/numerical regressions, feature-off and strict clippy, plus
ALOR 52-case / 4148-step parity are re-run. No source-review acceptance is claimed.

## Boundaries unchanged

Source-only; no managed auth/reconnect composition, worker join/exit witness,
Linux release or operational execution approval. No real FINAM calls, credentials,
VPS access, installation, timers, Redis, orders, live micro or main/origin writes.
The installed/running IMOEXF remains unchanged. Managed runner is the next slice
only after source acceptance. R0 immutable source and handoff remain retained;
cross-build migration/reopen is not authorized by this correction.
