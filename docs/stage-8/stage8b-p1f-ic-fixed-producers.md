# Stage 8B-P1-f Ic — fixed producers and retained high-water

Status: `REVIEW_CANDIDATE_FIXED_PRODUCERS_ONLY`.

Accepted predecessor: P1F-Ib correction R2 at
`7c481bc60699b514b016e8dffe62eb9ca462a100`, independently accepted by the
review whose SHA-256 is
`098a24fc87871968bc6e7b0a77deefffd403bc56d997d21af18e1869750acf7e`.

P1F-Ic adds only the fixed O3 synthetic and O4 FINAM-read-only schedule and
M10 producers needed by the accepted operational model. O2 fresh-source
materialization remains the single accepted P1F-Ia `materialize_o2` path and
the ordinary P1-e first-boot verifier; Ic deliberately does not create a
second first-boot source path or a provider framework.

## Fixed source composition

O3 receives an exact public synthetic fixture bound by its phase identifier.
The schedule fixture identity excludes observation timestamps but includes the
accepted semantic schedule identity. Its Stage4 and schedule observations must
be at most two seconds old and no more than five seconds apart. The ordinary
P1-e schedule normalizer, generation-2 signer and publisher are used unchanged.

O4 receives the existing typed GET/read-only FINAM schedule adapter input. It
requires a retained `Published` schedule high-water, accepts observations no
older than 30 seconds with at most five seconds of cross-source skew, and calls
the same P1-e normalizer/signer/publisher with `Resume`. O4 has no
first-publication authority.

Neither path contains a FINAM HTTP client. Local fixtures exercise the exact
typed adapter boundary; operational network attachment remains a later phase.

## Canonical M10 contract

Each candidate consists of exactly ten canonical broker-neutral final IMOEXF
M1 bars. O3 accepts only `ReadOnlyPoll` bars and requires the phase-pinned
fixture digest. O4 accepts only `LiveStream` bars and forbids a synthetic
fixture digest. O3 is a fresh bounded batch: every receipt is at most two
seconds old and the batch spans at most five seconds. O4 retains the actual
streaming receipt for every minute: receipts must be monotonic and no earlier
than their source close, while freshness is evaluated against the final M1
receipt that completes the M10. The completed O4 M10 is rejected when that
last receipt is future-dated or older than 30 seconds; earlier timely minute
receipts are not rewritten to the aggregation instant.

The existing `CanonicalBarAggregator` creates one 600-second bar. The result is
accepted only when its complete interval is inside a fresh signed
`TradableOpen` schedule. M10 admission runs the accepted P1-e signature and
freshness verifier at the current trusted instant, binds the verified exact
envelope bytes back to the retained publisher-state hash, and checks the
expected operational identity and current source expiry before creating
`Prepared`. The existing Stage 8B-P1 canonical M10 builder and parser produce
and cross-check the Redis identity, semantic identity, payload hash and exact
canonical bytes.

## Retained high-water and restart

The local state has only `Prepared` and `Published` phases and fixes source
generation `1`. It retains:

- producer phase and exact phase identifier;
- strictly increasing publication sequence;
- operational and schedule-envelope identities;
- exact source-M1 batch identity;
- canonical M10 Redis, semantic, payload and byte identities;
- exact canonical M10 bytes and, after publication, the exact Redis ID.

The only empty-state authorization is explicitly typed and is valid only for
O3. O4 must continue a retained O3/O4 state. An exact equal candidate replays
the retained `Prepared` bytes or returns the retained `Published` state. A
different candidate at the same close is a conflict; an older candidate is
stale; a newer candidate cannot pass an unresolved `Prepared` publication.
O3 to O4 advances the same sequence and cannot create a new genesis.

State persistence is canonical JSON written to a create-new mode-0600
temporary file, followed by file sync, rename, parent-directory sync and exact
reread validation. `Published` can be formed only after the caller presents
the exact deterministic Redis ID and exact retained bytes. The actual Redis
role and publish/reread operation belong to P1F-Id.

## Evidence and remaining boundary

Local tests cover the O3/O4 schedule sequence and semantic revision,
Prepared/Published restart, exact duplicate replay, same-close conflict, stale
input, prohibited O4 genesis, unresolved-Prepared blocking and O3-to-O4 M10
continuity. Dedicated controls cover ten sequential O4 receipt timestamps,
future/stale/non-monotonic completion, forged signature, changed schedule
payload, publisher-state/envelope hash mismatch, operational-identity mismatch
and signed-but-expired source evidence. The Ic gate also retains the accepted
schedule-publisher and O2/Ia tests.

P1F-Id fixed Redis roles, resource polling and command audit and P1F-Ie
aggregate source closure remain open. P1F-O0 through O4 and P1F-A remain
closed. This slice does not install or start a service, mutate a VPS, connect
to operational Redis, execute a paper provider, perform FINAM POST/DELETE,
dispatch a broker command, start runtime-live or place a real order.
