# WS-primary V6: bounded recent REST-tail recovery

Baseline: `dfc6f64d0a2517d9851040187e4d8729f02ab9a4`, tree
`57f55be3f0a0f76f4d9bd72728b0ce301a523738` (accepted V5 R1).
Scope: source review only. No installation, activation, one-shot retry, DB0,
broker dispatch, real order, runtime-live or live-micro authorization.

## Explicit policy and finite budgets

Opt-in `ObservedBoRestTailRecoveryV6` has a different configuration and policy
fingerprint. V1–V5 retain their accepted admission behavior, including the V5
terminal result for the November 12 fixture. Strategy and feedback parameters
are unchanged; `model.rs` changes only a helper inside `#[cfg(test)]`.

| Budget | V6 contract |
| --- | --- |
| Live admission plus publication | 2000 ms, including 100 ms uncertainty |
| WS boundary grace | 1000 ms, unchanged |
| Recovery episode | At most 45000 ms; no deadline renewal |
| REST source requests | At most **4 per episode**, across connection attempts |
| DataNotReady request backoffs | **1000 / 2000 / 4000 ms** |
| Individual source ticket | At most 5000 ms and never beyond episode/run/EOD |
| Historical publication | At most 10000 ms from admission, capped by episode/run |
| Connection attempts | Existing 3 with 1/2/4-second backoff |
| Episodes per run | Existing 3 |

The request count is a conservative upper bound on actual GETs: an issued ticket
still counts when STOP/auth prevents its GET. It is NOT a reconnect counter.
Only the existing auth routes can POST; order POST/DELETE remain closed.

## Non-destructive assessment

Before invoking the existing mutating recovery admission, V6 checks opaque
structural admission, current request/gap/owner/generation/frontier, timestamps,
ALL committed overlap slots and all unpublished cache/retained inventory.
True strict errors take precedence over missing-tail. Committed slot loss or
addition is never treated as a value-only revision.

`DataNotReady(MissingRecentUnpublishedTail)` is allowed only in the last closed
unpublished M10 immediately before the fixed cutover. Missing known slots must
be a suffix: no later REST slot may occur after the first missing known slot.
The union includes retained-only witnesses. A completely empty last bucket is
explicitly pending when overlap is intact. Earlier missing buckets, interior
known holes, invalid identity/range/OHLCV/hash and committed inventory errors
remain refusals. A valid sparse bucket covering all KNOWN slots remains valid;
there is no new dense 10/10 rule.

Pending consumes the individual response, but does not mutate model/book,
frontier, committed inputs, reference installation, retained authority or source
change authority. The raw source and assessment are diagnostic evidence only.
A timing-expired but structurally valid response has separate reason
`RequestExpired`; it never becomes canonical because of later processing.

## Request identity and controller route

Each new request binds config/policy/calendar, logical gap owner, generation,
range/cutover, frontier, issued/deadline times, episode, ordinal and a monotonically
increasing per-processor serial. The request SHA includes that scope and serial.
The opaque owner remains an in-memory capability, not a deserializable receipt.

Only a consumed, unadmitted current request may be renewed. Renewal retires the
old owner before creating the replacement; it cannot extend the episode, move
the frontier, change generation or reuse an old response. A later old completion
is recorded as stale and creates no callback; a current completion remains
usable exactly once. A new real WS fault invalidates the previous gap/tickets.

Writer propagates typed DataNotReady to the managed engine. The engine waits
using the current healthy connection while still polling ingress, STOP and
deadlines. It then asks the writer for a new ticket and sends another GET.
No reconnect is performed just to retry REST. After four incomplete requests:
`RestTailRequestsExhausted`; after episode deadline: `EpisodeDeadline`. Neither
condition opens another episode or restarts the session automatically.

Cutover must still equal the current closed-M10 boundary at request issuance.
Renewal never truncates already closed required bars. A boundary crossing is not
an implicit permission to retime or shorten a request; existing strict bounds
remain in effect. The incident's close-triggered 45-second episode cannot reach
the next ten-minute boundary.

## Same historical model/book

Before full batch admission: zero callbacks/intents/fills. After Ready: the same
owned session processes each missing label once as `RECOVERY_BACKFILL` /
`BACKFILL_REPLAY`, including legitimate historical BO paper entry/exit and candle
feedback. Overlap is validated but never replayed. Virtual close is distinct
from actual request/receive/model/publication times. The next live step continues
the same model/book; it is not reset to flat. Publication uncertainty after any
possible model mutation is terminal and cannot retry the callback.

The existing independent transport and canonical latches both remain necessary.
Restored historical inputs are never reclassified as timely WS/live signals or
broker execution authority. Allowed recovery does not itself prohibit semantic
EOD; it is counted separately from timely live delivery and operational quality.

## Producer and reader evidence

New records: `primary_recovery_request_issued`, `..._retired`,
`..._stale_discarded`, `primary_recovery_data_not_ready`, and diagnostic
`primary_recovery_source_observed`. Existing source-validation records bind the
request SHA and include V6 `validated_at_ms`.

The restart-prefix reader and full EOD reader share a file-only V6 lifecycle
check. It recomputes request hashes and inventory suffixes, checks deadlines,
read counts/backoff, source chronology, retirement and original commit bindings,
and rejects historical input from an expired/retired/Pending ticket. The normal
reader still validates actual source/step/feedback/commit, model transitions,
calendar, generation resolution and EOD. V5 files remain readable unchanged.

Compact `primary_source_readiness` transition snapshots preserve the true
predeadline reason, cached/current-generation bitmaps, quarantine, boundary/grace,
remaining budget, component RX/processing and preceding evidence-I/O times.
V6 REST observations add actual request/receive/processing and raw evidence-I/O
timing. These records do not relax admission or assert network readiness.

## Focused coverage / reviewer T1–T11

| Matrix | Evidence |
| --- | --- |
| T1 | Original 7/10 + 27/30 November 12 raw fixture; V5 HOLD vs V6 Pending |
| T2/T3 | Pending, new ticket after old 5s deadline, one backfill, same-generation continuation; late old response rejected |
| T4 | Managed 4-GET exhaustion with no extra reconnect; no gap callback |
| T5 | Empty latest bucket, retained-only tail, partial tail arrivals, valid sparse inventory |
| T6 | Interior hole, committed missing/extra slot, foreign generation, auth failure; intrinsic malformed/hash/OHLCV admission regressions retained |
| T7 | Existing actual-writer optional REST races extended to V5/V6; no late live input |
| T8 | REST-only wait retains socket; a new actual peer fault invalidates old ticket |
| T9 | Actual BO historical entry/exit/feedback and nonflat long/short, then same live book |
| T10 | STOP in backoff; uncertain source/step/commit evidence cannot reexecute model |
| T11 | Compressed full-day EOD, one historical label, new request lifecycle and resealed reader negatives |

Real fixture provenance: result ZIP SHA-256
`5216c09a6ed45eedd309897187803c2b5188c4a8f10e24bc60afd748efd81680`;
journal `2c22ea223a93e372b99472715951f937acccf96ae5a91ca526c1703ccdc3e1e6`.
`tests/fixtures/rest-tail-20261112.json` preserves the real Proof/Recovery bodies,
normalized known WS cache and overlap, not original WS wire receive times.
Cache timing reconstruction in the pure test is explicitly synthetic.

Every positive later-complete response, local peer and compressed full-day run
is SYNTHETIC. No later real FINAM reply, provider lag duration, full-session
observation, ALOR parity or clean-session credit is claimed. The original
November 12 result and accepted V5 source bytes are not rewritten.
