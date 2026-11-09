# Candle-only BARS heartbeat: source correction

Baseline composed tree: `c0db2d630d39c095805d5b26239a7d5e93b5c28f`.
Scope: BARS socket used by the candle-only runner. Source review candidate.

The 02 November fresh-session attempt stopped at 08:26:46.628 MSK with
`Receive / Deadline` after eight consecutive LIVE M10. The accepted client
used its 60-second BARS DATA silence budget as the socket wait bound.
The trace does not establish whether the provider, the network, or quiet
market conditions caused the silence. REST auth rotations had succeeded.

## Behavior

- Send a WebSocket protocol Ping every 30 seconds. Allow one outstanding Ping.
- Start the 15-second Pong allowance only after successful Ping send/flush.
  Require the corresponding payload. Wrong or unsolicited Pong, server Ping,
  other DATA and subscription events cannot clear the outstanding Ping.
  Reject a matching Pong received at/after its deadline.
- Keep a separate 70-second transport idle bound. RX timestamps include
  valid WebSocket Text/Ping/Pong, and never establish BARS/model readiness.
- At 60 seconds without admitted live provisional BARS, emit a warning once
  per silence episode. A matching Pong permits transport observation to
  continue. A later live BARS frame emits a resumed diagnostic.
- At 1200 seconds without live BARS, fail with `BarsDataSilence`, even when
  transport Pong keeps arriving. Initial history/empty BARS/auxiliary traffic
  do not refresh the live-data clock. Planned healthy reconnect does not
  erase the market silence clock.
- All elapsed-time limits use Tokio's monotonic clock. UTC values are evidence.
  Ping send, subscription send and server-Pong flush have the existing derived
  I/O cap (min of 15 seconds, configured stop timeout, configured WS budget).
  The shared stop latch and overall run deadline also bound these operations.
- Any error/cancellation drops the socket owner; no retry is introduced.

The accepted `ws_silence_budget_ms` remains the legacy/data validation input;
it is not silently changed in a signed config. The candle-only implementation
has the fixed 60/1200/30/15/70 policy above, versioned by its source identity.
Legacy and QUOTES profiles retain their existing data-silence behavior.
Current parser timestamp bounds still use the original configured budget.

Canonical M10 continues to come from independently scheduled admitted REST
M1 snapshots. Calendar, sparse rules, timestamps/OHLCV, dedup, model frontier,
Lmax, EOD cutoff and close-fill feedback are unchanged. Neither heartbeat nor
provisional BARS grants finality or excuses a missing/stale required M10.

## Evidence

`bars_liveness` transport records carry typed cause (PingSent, PongConfirmed,
BarsDataSilence, BarsDataResumed), connection/subscription-auth generation and
the latest RX/DATA/Ping/Pong times with monotonic ages. A Pong record explicitly
does not claim market readiness. Payloads and credentials are not recorded.
First-failure receipt schema 2 retains the typed error and a bounded liveness
snapshot independently of the event queue. Timeout kinds distinguish Pong,
transport idle, market silence, Ping send, server-Pong flush and run deadline.
`runner_end` additionally records the last successfully accepted canonical M10
label and received time for this invocation (null before its first snapshot).

## Deferred resilience iterations

Reconnect/resubscribe should reuse the ALOR principles, in a later reviewed
slice: finite attempt/time budget; provider-specific fresh authorization and
handshake/subscription; connection generation fencing; REST validation of a
bounded gap with overlap/dedup; resume from the saved model frontier without
repeated callback/virtual fill; explicit recovery-versus-LIVE provenance.
Transport liveness, subscription readiness and model freshness stay separate.
Do not copy ALOR's unbounded reconnect loop, GUID ACK or CWS trading protocol.
Source acceptance of heartbeat will not authorize these deferred mechanisms.

## Verification and next boundary

Paused-clock tests exercise actual duplex WebSockets with matching Pong,
quiet market >60 seconds and the 1200-second hold. Negatives cover missing,
wrong and late Pong; server Ping; stalled writes; stop/deadline; error/EOF;
stale DATA and retained legacy/QUOTES behavior. Existing sparse/candle/state/
feedback regressions run with the affected crate suite.

This source candidate does not establish native Linux release qualification,
FINAM Ping/Pong compatibility, a new operational run, full EOD or ALOR parity.
Next: source review, then release qualification and an explicitly scheduled
observation. Existing failed evidence remains unchanged.
