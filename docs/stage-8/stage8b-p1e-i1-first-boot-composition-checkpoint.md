# Stage 8B-P1-e I1 first-boot composition checkpoint

Status: source review candidate for the F00-F17 first-boot composition only.
This is not complete I1 supervisor acceptance and not operational activation.

Accepted predecessor:
`8360c4701b6abbe75ced988cf8dd2d74487e1846` (I1A source correction,
independently accepted on 2026-09-13).

## Implemented boundary

The checkpoint connects the accepted immutable supervisor/profile boundary to
the existing Stage 5C, Stage 5D, Stage 5G and Stage 8B-P1 durable bootstrap
facades. It adds no Redis or FINAM dependency.

The production path now performs:

1. F00: read the one fixed first-boot source path with `O_NOFOLLOW`; require a
   regular single-link file, root ownership, the configured service group,
   mode no wider than `0640`, a bounded 16 MiB complete read, stable metadata,
   duplicate-key rejection and the configured source SHA-256.
2. F01: construct the fixed Hybrid IMOEXF runtime profile and compare its
   actual configuration fingerprint with the validated bootstrap profile.
3. F02: construct broker-neutral Stage 4 truth for the configured account and
   IMOEXF instrument, then require complete fresh flat position/order truth.
4. F03-F04: perform the accepted Stage 5C bootstrap and canonical final
   History M10 warmup with `FinamDerivedM1ToM10` provenance.
5. F05-F09: independently replay the exact History bars through the source
   High180 riskgate kernel, compare every supplied observation byte-exactly,
   derive the Stage 5D ledger/materialized state through accepted algorithms,
   validate it and apply only the rebuilt runtime state.
6. F10: prove an empty bounded recovery set for ACK, order, stop-order and
   position streams. This is deterministic input composition, not Redis
   consumer activation.
7. F11-F14: process exactly one later-session Replay M10 candidate, require
   one callback with zero intents/requests and no execution eligibility,
   derive the current shadow state and construct the clean-restart export
   input.
8. F15-F17: pass the resulting source/export/fresh-runtime tuple to the
   existing authenticated export/restore and durable-root transaction. An
   integration test proves one identity-derived durable directory and a normal
   restart without a second first boot.

"First candidate after history" means the first candidate consumed by this
one-shot Replay phase after the completed History tail. It deliberately does
not require `candidate.close_time == history_tail.close_time + 600`: a closed
market, overnight gap or weekend may separate the two sessions. The candidate
must be a final aligned M10, must be in a Moscow session strictly later than
the History-tail session, and is consumed exactly once.

## Evidence in this source slice

- 121 complete History sessions are supplied and independently replayed;
- all 121 finalized source observations are compared, while the accepted
  riskgate minimum remains 120 sessions;
- a mutation that changes an observation and recomputes its outer hash is
  rejected by the independently rebuilt High180 oracle;
- source hash, deployment/account/profile binding, duplicate JSON keys,
  freshness, OHLC validity, chronology and candidate-session failures are
  fail-closed;
- the full first-boot test reaches authenticated export/restore before the
  sole durable-root creation and then completes an ordinary restart;
- the resulting receipt keeps Redis and FINAM transport unattached.

The modified-crate suites, doctests, formatting and strict all-target/all-feature
Clippy pass. The inherited Stage 7B Redis subprocess suite passes completely
when serialized. A concurrent aggregate run observed one claim-barrier timeout
in the inherited X16 test; the exact test and the complete subprocess target
both passed when rerun with `--test-threads=1`. This timing observation is not
silently converted into a green parallel-run claim.

## Remaining before complete I1 acceptance

This checkpoint intentionally does not claim the following work:

- first-boot transaction V5, receipt V2 and provenance binding, including
  SIGKILL/response-loss adoption evidence;
- the deployable binary and single-owner process loop;
- the accepted Ready/pending/timer route matrix and the split
  `bind -> shutdown observation -> guarded resume E -> F -> effect` path;
- process signal, panic, owner-loss precedence, bounded drain and exit-code
  evidence;
- exhaustive restart/process evidence proving no repeated callback or effect;
- systemd/install packaging and a final aggregate I1 acceptance package.

These items must be implemented and reviewed as later I1 slices. This
checkpoint must not be described as deployable I1.

## Closed surfaces

The following remain closed: operational Redis DB0/DB15, VPS activation,
operational private-key installation, FINAM POST/DELETE, broker dispatch,
runtime-live and real orders. The source bundle contains observations only and
cannot supply serialized runtime state, a materialized riskgate ledger or an
effect authority.
