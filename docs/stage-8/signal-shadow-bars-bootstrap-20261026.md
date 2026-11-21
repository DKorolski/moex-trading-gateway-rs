# BARS row-level bootstrap / source candidate

Accepted predecessor: c70bc9a48057d27f93027773c8f01d648559d42d.
Scope: REVIEW_BARS_TIMESTAMP_SMOKE_c70bc9a4_AND_BOOTSTRAP_SCOPE_ACCEPT_RU.
Source candidate only; no installation, network probe or operational qualification.

## Wire contract

FINAM [marketdata proto](https://github.com/FinamWeb/finam-trade-api/blob/main/proto/grpc/tradeapi/v1/marketdata/marketdata_service.proto)
defines proto3 Bar.is_data_snapshot (bool, field 7) in repeated bars.
The [ProtoJSON contract](https://protobuf.dev/programming-guides/json/)
allows original snake_case and lowerCamelCase field names and omits default
scalar values. This boundary explicitly selects that contract: absent flag is
false, explicit boolean true is history, false is live. Null, non-booleans and
simultaneous snake/camel aliases are rejected. No age/count/first-frame heuristic.
This is a documented compatibility assumption for the FINAM WS payload,
NOT observed wire evidence. The accepted smoke did not retain this flag.
Before operational qualification, separately authorize and retain a safe
presence/type/true-false projection to confirm the actual provider wire format.

## Implementation

Typed row flags stay alongside mapped rows, with exact mapped-length check.
No changes to REST DTO, neutral Bar, strategy, feedback, seed or REST admission.
All timestamps and OHLCV are validated before any data/key/quota commit.
Only the age predicate is skipped for explicit snapshot=true.
All live predicates remain unchanged; stale live cannot become history.
InitialHistory holds only safe numeric history bounds/count and the validated
live subset. Snapshot bars themselves are discarded and never gain finality.

Each owning physical connection allows at most 512 history rows and 256 KiB
aggregate bytes before its first live DATA. Byte accounting conservatively
charges complete wire frames containing history, including mixed-live overhead.
This can reject earlier than payload-only accounting, never later.
Mixed initial frame commits atomically and closes bootstrap when live is present.
Later history is UnexpectedSnapshot. Any validation failure holds the protocol.
Auth rotation resets quota/live-seen only; the accepted silence anchor and
QUOTES dedup remain unchanged. Old connection tokens cannot mutate new state.

Both diagnostic and S5 workers emit snapshot_discarded receipts with generations,
rows/min/max timestamps and data_items=0. Only a nonempty live subset can emit
first_validated_data or advance the controlled-reconnect frame counter.
Coverage records history without granting g1/g2 readiness.
History/heartbeats do not renew silence or authorize reconnect.

## Local evidence

Synthetic 300-row fixture follows observed geometry, not retained provider raw.
Tests cover mixed atomicity, fresh history, stale/missing/invalid flags, aliases,
timestamp/OHLCV failures, row/byte split caps, late snapshots, auth quota reset,
owner fencing, unchanged silence and redacted receipts.
Duplex WebSocket history/cap/stop fixtures use the production Session and feed
the diagnostic coordinator: failure remains exit 1, both workers joined.
Retained QUOTES/auth/credential/flush regressions must pass unchanged.

No claim of operational rotation or S5 success. Next boundary: source review,
then release qualification and separately authorized wire/operational checks.
