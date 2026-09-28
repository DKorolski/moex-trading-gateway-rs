# Stage 8B-P1F ALOR → FINAM source correction

## Decision

The migration target is the operating ALOR `baseline07` BO-only model. The
`candidate09` fixture is retained as a separate research reference and is not
silently promoted to the FINAM target.

The freeze is accepted as model evidence, not as an operational FINAM source
bundle. Its 63 weekday dates do not satisfy the existing 121-session first-boot
contract, and its M10 rows must not be expanded into invented M1 provenance.

## Corrected semantics

- broker-facing MR entries are disabled before owner or pending state can be
  acquired; High180 shadow accounting remains independent;
- the model session starts at 07:00 MSK and excludes weekends;
- a canonical FINAM M10 keeps its close-bound identity and Redis stream ID, but
  the strategy callback receives the candle-start model label;
- availability/callback time remains separate and cannot precede M10 close;
- same-day BO exits remain executable at 23:30 model label, while new entries
  are prohibited at and after that label;
- the accepted successor-open paper provider is unchanged. Current-close fills
  exist only in the explicitly labelled zero-slippage diagnostic replay.

The historical field name `BarEvent.close_time_utc` is therefore not used as
proof of close-bound model time. At the FINAM semantic bridge it carries the
strategy model label, while the validated canonical envelope retains exact
open, close, M1 provenance and close-derived Redis identity.

The same convention now applies to production first boot: History warmup,
High180 shadow reconstruction and the Replay candidate receive candle-start
model labels. Canonical source bars, availability, hashes, receipt ranges,
watermarks and Redis identities remain close-bound. A linked regression crosses
History → Replay C0 → authenticated export/restore → adjacent C1 → replacement
export/restore and includes the 23:40–23:50 MSK History boundary.

## Evidence

The fixture-backed Rust test sends all eligible bars through
`HybridIntradayRuntimeStrategy`, applies deterministic current-close position
callbacks, and compares complete position rounds to the frozen Python reference.
It consumes 7,307 raw M10 rows and matches all 38 baseline07 rounds on side,
entry/exit labels, entry/exit prices and normalized exit reason.
This is FINAM Rust runtime versus the frozen Python baseline07 reference under
diagnostic current-close fills; it is not a claim of full ALOR Rust executable,
broker-lifecycle or successor-open execution parity.

The comparison helper is hardened against empty evidence, `NaN`/non-finite
prices, wrong profiles, wrong source data and row-count drift. Candidate09 is
validated as a frozen input but is not treated as a target acceptance result.

## Artifact boundary

The earlier O2 artifact at `1090de4` remains useful as a reviewed/implemented
infrastructure checkpoint, superseded for the target; artifact acceptance was
not established. It binds the previous High180/09:00 profile, is not a final
artifact for the corrected target and must not create an operational root.

The replacement source aligns O2 custody without adding capabilities: the root
runner uses primary group `moex-p1-paper`, `UMask=0027`, and the guardian applies
the exact reviewed mode with `fchmod` before `fchown`, validation and publish.
After force-kill, both the primary runner and `ExecStopPost` use the same
45-second monotonic stopped-proof budget; expiry returns nonzero
`StopNotProven` and writes no successful terminal receipt.

After review of this source correction, O2 identities, source schema/template,
binary and non-activating installation package must be rebuilt against the new
profile hash and runtime fingerprint. Only then may limited paper windows begin.

No Redis DB0/VPS activation, FINAM POST/DELETE, broker dispatch, runtime-live or
real-order authority is opened by this correction.
