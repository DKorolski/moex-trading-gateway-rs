# USD/RI real-input compatibility: source correction

Parent: accepted `f19eb207847510a5d26df900d46c2a1e2435801c`.
This slice changes admission, not numerical models, runtime/reconnect/auth,
order transport, CI or any installation. RI is the pinned RIZ6 contract; no roll.

## Known field and unchanged raw evidence

FINAM's [Bar proto](https://github.com/FinamWeb/finam-trade-api/blob/main/proto/grpc/tradeapi/v1/marketdata/marketdata_service.proto)
defines boolean `is_data_snapshot`. The public export boundary now accepts this
optional boolean (snake/canonical camel spelling), never both spellings at once.
Null, string, numeric flags, unknown row/decimal/body keys remain rejected.
The flag is not finality, receipt, completeness, LIVE or readiness authority.
Bodies/hashes are retained unchanged; no snapshot field is stripped from raw.

## Seconds, transport minutes and nominal M10 are different grids

Historical Schedule validates the original whole-second boundaries, ordering,
classes, conflicts, full horizon and hashes. It no longer requires minute-grid
boundaries; subsecond boundaries still refuse. No Schedule timestamp is rounded.

An admitted minute `[m,m+60s)` must have positive half-open overlap with a proven
Trading/Auction interval. This permits a real 06:59 M1 intersecting an auction
06:59:01–06:59:50, not a wholly Closed minute or a mere endpoint touch. M1 cannot
attest which seconds supplied its OHLCV. No seconds-based splitting is invented.

GET ranges must cover all prior trading/auction windows, and the existing builder
still requires a snapshot covering the entire nominal M10 for aggregation. A GET
only for 06:59–07:00 cannot stand in for the full 06:50–07:00 bucket. Keep every
admitted nonempty M10 in exact inventory, including off-model labels: no padding,
relabeling or silent auction/weekend omission. Existing numerical eligibility
then quarantines pre-07 weekday/RI off-model bars. USD's weekend-context arm may
use actual weekend 09:50 context; it must not be removed as apparent pre-open.

## Explicit initial-seed horizon, approved by the operator on 20 November

The historical seed ends at 00:00 Moscow on the model day, not its 07:00 freeze.
All prior civil days/weekends require independent calendar and query coverage.
Evidence may end exactly at midnight; actual acquisition/admission clocks may
occur later, but still must precede freeze. The previous 23:50–00:00 M10 remains
representable. A right-endpoint M1 stays in raw/accounting, never a seed candle.
Current-day pre-open bars are neither historical inventory nor model slots.
Current model-day input starts at its existing 07:00 grid through the unchanged
WS-primary V8 path, including its already permitted REST bootstrap/recovery
exceptions, not by importing it into this historical seed.

This removes an impossible requirement to query today's future auction while
collecting at 06:45. It does not exempt missing prior-day/weekend data. The
existing freeze watermark remains a DECLARED model cutoff, not attestation that
raw current-day pre-open was captured; no numerical contract/feedback changes.

## Tests and real component evidence

Eight tests cover known optional flags and invalid aliases/types; unchanged OHLCV;
transport/identity/range rejection despite a true flag; whole-second vs subsecond
calendar boundaries; mixed auction minutes and endpoint/Closed refusals; clipped
M10 GET and omitted nonempty inventory; today's auction exclusion; prior-day
midnight/right-boundary behavior; and two untouched real 19-Nov GET collections.

Real 19-Nov component fixture: USDRUBF 931 M1 / 101 M10 (37 sparse), RIZ6
985 M1 / 101 M10 (16 sparse). Original 19/20-Nov Schedule snapshots and source
collection hashes accompany the unchanged bodies/request/receipt clocks. The
typed snapshot uses the existing canonical `@` endpoint; original `%40` wire URL
is recorded separately. This is URL spelling normalization, not raw transformation.

These real fixtures prove schema, historical calendar and closed native snapshot
compatibility, NOT a complete READY two-anchor Monday seed. Friday/weekend data,
fresh metadata/account/auth and future runtime admission remain required.

The USD/RI implementation fingerprint changes normally; no legacy waiver or
old-generation reacceptance. The already sealed IMOEXF f19eb207 execution package
is unchanged. After SOURCE ACCEPT: exact Linux release, then separately reviewed
and operator-authorized bounded execution. No install/arming/WS run in this slice.
