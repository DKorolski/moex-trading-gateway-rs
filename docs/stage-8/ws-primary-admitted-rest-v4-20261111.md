# WS-primary V4: whole admitted REST before commit, historical bootstrap only

Status: source implementation for independent review, not accepted deployment.
Parent: `0002e927b6c3d18afe133e885f1309a9e3c5d29c` (accepted reader R1).
Installed runtime remains publication `3671bae9d683e5ade7c8e691274b13c7b4a57da2`,
tree `600e385ba68eae6edaeb8b5aa3fee734e695f751`; it is NOT this source snapshot.

## Evidence and decision

11 November long shadow stopped after four M10 commits. Public result ZIP SHA:
`debd29b35ece3a5f06fdacc6cb124e7a8b735ca9edb78008fdab334f8c06af55`.
At M1 07:49 MSK WS close/volume was 2450/2256; REST was 2450.5/2238.
All ten known minutes were present; the proof GET completed in 34 ms and
comparison left 860 ms before its deadline. V3 rejected the nonstartup REST
source because price changed AND volume decreased. WS boundary remained
unproven, then the existing 2-second fence started continuity recovery.
Physical generation 2 subscribed; admitted bootstrap REST completed in 183 ms,
but the same historical WS/REST close mismatch caused ReferenceConflict.
This was not a timer firing, authentication outage or evidence of no trades.
Broker-side cause of the revision and impact on strategy are NOT established.

The user authorized a narrow source correction after this diagnosis. Neither
increasing retries/timeouts nor rewriting already processed bars was authorized.

## Explicit opt-in, preserved legacy behavior

New delivery enum: `ObservedBoAdmittedRestV4`. No default changes. V1, V2 and
V3 serialized bytes, fingerprints and source-admission semantics remain intact.
Old executables reject the new enum; configuration/policy/semantic fingerprints
separate this policy from prior immutable evidence. Model and feedback stay
`imoexf-baseline07-bo-only-no-riskgate`; no quotes dependency is introduced.

V4 retains eligible WS-first, nonblocking optional REST comparisons and the
existing hourly/sixth-commit audit cadence. If the next slot is unpublished and
WS is not eligible, a timely whole admitted REST bucket may supply an existing
StartupPartial, SparseUnproven, BoundaryUnproven or SessionEnd exception.
Provisional WS price/volume disagreement alone does not veto that whole source.
Its origin remains REST_EXCEPTION, never WS_LIVE. No cross-source M1 mixing.

All source admission remains mandatory: exact instrument/request range, complete
transport/body, valid closed OHLCV, all known minutes present, original request
and receive timestamps, slot/reason binding, single ticket, no in-flight callback,
freshness and rechecked publication deadline. A ready WS bucket still wins;
late/unavailable/corrupt/incomplete mandatory data creates no permission.
After commit REST remains diagnostic and cannot replay/rewrite the model.

The 1-second grace, total 2-second live budget including 100-ms clock uncertainty,
5-second bootstrap/proof caps, queue/cache limits, recovery count and auth policy
are NOT enlarged. A missing reliable source still degrades or stops safely.

## Historical bootstrap

Only V4 may discard a differing historical WS row covered by an independently
admitted closed REST reference with the same instrument, timeframe and exact
minute boundaries. The row must have closed at its ORIGINAL frame receive time,
the generation must still be in bootstrap and the minute must be uncommitted.
Evidence context: `WS_BOOTSTRAP_REFERENCE_DISCARDED`, retaining old/new values
and original WS frame hash. The reference is not overwritten with WS data.

Discarded history adds neither cache data nor live readiness nor a model callback.
A mixed frame's live tail keeps its original receive time and the existing ingress
age limit. Foreign ownership/generation, wrong identity, absent reference, invalid
OHLCV/future data and committed price conflicts remain refusals. Existing
volume-only diagnostics remain unchanged. Reference-to-reference conflicts and
canonical recovery overlap checks are NOT relaxed.

This avoids reconnects whose only purpose would be reconciling provisional
versions before commit. Genuine socket/liveness faults still use the existing
bounded reconnect/resubscribe/recovery. No reconnect framework is added.

## Verification and limits

Runtime continues to emit an existing admitted canonical capability; the file-only
restart/EOD verifier already independently re-admits the full inline REST evidence,
checks canonical OHLCV, hashes, slot, policy binding and timing. No verifier bypass
or producer PASS flag is introduced. A full synthetic 101-label managed-WS day
includes a nonstartup close+volume REST replacement, follows with same-generation
WS data and natural EOD, then passes the independent Rust verifier. Mutated source
provenance and canonical price fail even after resealing the journal hash.

The retained numeric pair fixture records the actual archive SHA and original
timestamps. Tests intentionally use synthetic clocks and other M1; they are NOT
a reconstruction of unretained WS receive timings or a provider/latency soak.
Old source/refusal tests still run. Separate negative cases cover missing known
minutes, late/failed REST, duplicate tickets, WS-first/no rewrite, strict committed
prices, bootstrap missing reference/identity and stale mixed-frame live tails.

No VPS/installed ELF, operational inputs, timers, real account/credentials, Redis
DB0, broker order routes or live micro are modified/activated by this source work.
No clean session or ALOR parity is claimed. The consumed 11-Nov run ID cannot
be reused. After source acceptance: new Linux release and a separately authorized
execution package explicitly selecting V4. Do not patch or relabel old evidence.

The old execution wrapper's fixed final success-looking export message is a
presentation limitation; state/result.json and runtime completion are authoritative.
The next execution package should display the actual classifier status. This
source slice does not silently edit the already accepted wrapper/units.
