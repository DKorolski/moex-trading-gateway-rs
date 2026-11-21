# Typed admitted producer → numerical models → SQLite: first source slice

Accepted predecessor: catalog/mapping `6a508df3bedf984de16439a4332971f287af054b`.
Numerical models and previous local journal acceptance remain valid. This
delta is a SOURCE REVIEW candidate, not an operational or Linux release.

## What is implemented

An opt-in `finam-signal-shadow/models-bridge` feature adds
`primary::models_bridge::ImoexfModelBridge`. The default feature set is empty.
No existing CLI/runner activates the bridge. No network connection or process
is started by create/open/consume.

The existing native V6 producer is still IMOEXF-only. Therefore this first
integration accepts **IMOEXF@RTSX only**, two independent accepted policies of
the Author41-short + Hybrid BO053 combo. USDRUBF and RIZ6 keep their accepted
numerical kernels and mapping; their native producer integration is NOT done.
The other four local arms are initialized, but receive no fabricated inputs.

Consumer order:

1. Take the non-Clone/non-Deserialize `AdmittedCanonicalM10` by value.
2. Check the exact pending owner, source frontier, generation and unchanged
   V6 deadline before model/SQL work. Bind exact source config and policy.
3. Check the accepted mapping artifact SHA, tick 0.5, trade lot 1 and exact
   session slot. Reaggregate the actual admitted M1 using Decimal and compare
   its runtime projection/bitmap against the capability.
4. Persist the exact source receipt, its digest, any REST evidence, numerical
   input, both decisions, marks, both checkpoints and chain in the SAME
   existing FULL-sync SQLite transaction.
5. Reread input/outcomes/checkpoint checksums and cross-links in a coherent
   read transaction. Only then call `PrimaryProcessor::committed`, which
   repeats owner/frontier/deadline checks.

The shared writer is reused, not copied. No extra outbox, recovery service,
external publisher, Redis XACK or broker dispatch is introduced.

## Evidence and clock boundary

The native capability carries source admission, NOT observed context/model
session acceptance. Initial contexts still use the accepted declared-clock
replay contract. All numerical outcomes remain `declared_clock_replay`,
`observed_clean_session=false`, `order_emission_allowed=false`.

`DeclaredCanonicalProjectionV1` stores source provenance without inventing a
raw HTTP body. A serialized receipt is UNATTESTED JSON in the local journal;
it cannot construct a native capability or alone advance a source processor.
Synthetic root tests exercise the actual admission implementation using
test-only producer fixtures. They are NOT market observations.

Original receipt timestamps (subscription, M1 receive, boundary/proof receive,
canonical-ready) remain unchanged in provenance. For the old numerical DTO,
`received_ms` is the actual capability receipt; `durable_ms` and
`source_processed_ms` are DECLARED numerical availability at canonical-ready.
They do NOT claim upstream persistence or actual callback/fsync completion.
This explicit clock basis is part of the hashed projection. The separate
SQLite write-prepared timestamp also does not claim to be the commit instant.
There is no shift of historical clocks to make freshness pass.

## Retry, crash and failure

Exact redelivery after reopen returns the same revision/result IDs without
stepping the model again or returning marks. Changed source proof, receipt
clock, generation, mapping, config or slot is not silently treated as exact.
Continuation after checkpoint restore preserves accepted pending/position
semantics. Reopen internally replays the causal log for validation; that is
not publication or external callback re-execution.

Any consume error holds this consumer until explicit reopen/inspection.
Failure before commit advances neither SQL revision nor source frontier.
If SQL committed but later readback or deadline completion fails, output stays
durable and the producer is NOT advanced; the error explicitly reports
`DURABLE_OUTPUT_SOURCE_NOT_ADVANCED` for failed source completion. No rollback,
fresh re-emission or relaxation of the two-second V6 fence is claimed.

This proves exact same-receipt retry only. It does NOT solve physical socket
restart with a different generation/receipt, transfer of an in-flight
capability, continuous-session rollover or fully observed context admission.
Those remain separate bounded source slices; numerical formulas need no
reaudit unless a regression is found.

Tests include native owner/config/mapping/policy rejection, conflicting slot,
deadline failure before and after SQL, continuation with restart, and a real
SIGKILL after durable commit before source completion. The existing four
SQLite atomic SIGKILL frontiers remain in the standalone regression suite.
Portable readback tests also mutate outcome indexes/checkpoints and check
stale-writer rejection.

## Catalog helper follow-ups, without recollection

CM-N1: require a list of strings for account IDs, with exact selected-account
membership (not substring/container membership on malformed JSON).

CM-N2: collector and offline verifier now share target subset selection.
Reject an additional row using a selected full provider symbol or asset ID,
and reject collisions between selected targets. This is NOT a full registry
audit or a requirement to recollect 63,000 rows.

The accepted mapping JSON remains byte-for-byte unchanged, including its
historical PASS_PENDING_REVIEW field and old collector/verifier hashes.
Revalidation with the corrected verifier changes only its current helper
hashes; this must not be described as a new network collection.

## Reproduction

From the extracted source root, with Rust/Python and cached locked crates:

```sh
python3 -B scripts/finam_shadow_admitted_bridge_verify.py --output /absolute/new-checks --target-dir /absolute/build-cache
```

The verifier checks byte identity of pinned model kernels, profiles, accepted
mapping and unchanged source/runner files using the baseline blob manifest.
It runs catalog tests, standalone models in debug/release, native default and
opt-in unit tests, compile-fail doctests, formatting and strict clippy.
No GitHub Actions round trip is needed for this source qualification.

The optional dependency is excluded from automatic root workspace membership
to preserve its standalone numerical workspace. Root Cargo.lock adds only
the local package/path edges; no registry versions change. Enabling the
feature unifies serde_json/float_roundtrip as required by accepted checkpoint
semantics. Disabled/default builds do not link the new model consumer.
The root and standalone numerical lockfiles have different dependency
resolutions (including serde_json 1.0.150 vs 1.0.151). Both are kept unchanged;
the bridge provenance implementation hash additionally pins its crate manifest
and ROOT Cargo.lock. A dependency-graph change cannot silently reopen the same
bridge journal under the old consumer contract. Standalone source-only model
fingerprints are not presented as native ELF/build fingerprints.

## Next boundaries

After this review: explicit native USDRUBF/RIZ6 producer routes with their
accepted identity/tick/contract rules, then observed context and session
continuation requirements, followed by separate release/execution acceptance.
Do not claim all six branches operational merely because mapping is accepted.

VPS/installed ELF/P0/O3, WS subscriptions, operational DB0, FINAM order
POST/DELETE, broker dispatch, runtime-live and real orders remain untouched.
No strategy parameters, feedback, virtual quantity or fees change.
New clean observed sessions: **0**.
