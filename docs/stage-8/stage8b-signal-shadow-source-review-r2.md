# Signal-shadow R2 — narrow R3/R4 correction

Review predecessor: `e1d5e8b3da38c61eb1d6cbe7cdb1b4a5718fba40`.
Request: `reviews/REVIEW_SIGNAL_SHADOW_e1d5e8b3_R1_RU.txt`.
Status: PENDING INDEPENDENT SOURCE REVIEW, not operational authorization.
Exact candidate identity and completed checks are in the package evidence.

## R3: quote precision

ObservedSession now keeps an immutable strict cross-prefix quote floor separate
from its nondecreasing millisecond live frontier. QuoteInput still owns exact
full-precision deduplication and conflict detection. Distinct admitted quotes
in the same millisecond do not stop the session. This does not authorize fills
at or before the decision timestamp.

The composition regression normalizes distinct submillisecond source events,
feeds their raw frames into ObservedSession both flat and with an actual-model
pending intent, verifies pre-decision no-fill and at most one feedback effect.
Existing strict prefix-floor and conflict regressions remain required.

## R4: independent receive queues

Prefix reconstruction keeps the global monotonic result/applied effect clock,
but no longer compares a channel's receive time with the other channel's prior
effect. Each receive must be within the observation range and no later than
its own effect. REST receive ordering is checked separately; QuoteInput retains
WS receive/source ordering. Raw evidence and timestamps are not rewritten.

Two queue-overlap regressions compare reconstructed model state, pending,
position and side-used with uninterrupted actual-model processing, including
the pre-decision no-fill case. Clock/range/coverage refusals and BACKFILL-only
classification remain enforced.

## Unchanged scope

The accepted historical oracle, trading predicates, fixed profile, EOD slice
and O4 backlog are unchanged. The September 27 ALOR freeze is a historical
reference, not evidence of current live equivalence. Prefix promotion still
requires separately trusted inventory approval. No VPS, credentials, Redis,
FINAM observation, installation or live-micro action is included.

The isolated source is tested before sealing. The source commit is a local
object only: no branch/index movement, push or merge. Archive SHA, byte manifest,
safety and predecessor diff identify the review target. Native Linux/MSRV and
two observed sessions remain outside this source-only correction.
