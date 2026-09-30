# ADR: BO-only paper target without mandatory riskgate history

Date: 2026-09-30.
Status: accepted project-owner direction; the
[source correction](../stage-8/stage8b-p1f-no-riskgate-source-correction.md) is
independently SOURCE ACCEPTED at `2f46491a4f63249f64676306d5f9c3445818f4f9`.
Authority closure/CI and artifact gates remain separate. This decision record
grants no activation authority.

## Context

The project owner confirms that current ALOR systems do not use riskgate.
This is the current operational requirement, not an independently collected
ALOR live snapshot. The September 27 freeze remains immutable historical evidence:
its IMOEXF baseline07 config still retains High180 shadow accounting, with MR
entries disabled; its candidate09 config explicitly disables riskgate. The
freeze guide separates that accounting from BO signal parity.

FINAM already uses baseline07 BO-only entry semantics, but its accepted first-boot
source still requires 121 history sessions, at least 120 riskgate observations
and High180 reconstruction. Its materializer enforces a minimum 180-day query.
These are real dependencies of the installed binary, not the current model's
operational warmup requirement. The September 30 successor O2 rejected a missing
April 10 minute before bootstrap. Retrying that same long-history contract is
not the next development step.

The [handoff reconciliation and scoped implementation plan](../stage-8/stage8b-p1f-bo-only-handoff-alignment-2026-09-30.md)
records source identities, model differences, actual configuration and checks.

## Decision

1. The immediate FINAM target stays **IMOEXF baseline07 BO-only paper**, with
   MR entries disabled before ownership/pending acquisition. Candidate09 remains
   a separate research/paper model, not an implicit replacement.
2. For this target, riskgate is explicitly disabled. High180 shadow accounting,
   lb120, seed import and riskgate ledger reconstruction are not admission or
   readiness prerequisites. An optional future riskgate model requires an
   explicit model decision; it is not on the critical path to paper/live micro.
3. Separate strategy-history warmup from riskgate history. The IMOEXF ALOR
   gateway's four-session configuration is the short-warmup reference. Admission
   must prove the required previous-session anchors and, where applicable, the
   closed current-session prefix under the selected calendar. Four sessions are
   not four bars or necessarily four calendar days. Missing required recent
   data remains a failure; no fabricated candles, arbitrary skipped gaps or
   automatic fallback to a multi-month riskgate rebuild is allowed.
4. Do not globally delete riskgate infrastructure, historical tests or accepted
   evidence. Introduce an explicit versioned no-riskgate profile/source contract;
   preserve validation for legacy riskgate-enabled data. Bind the successor to
   its own profile/hash/fingerprint and reviewed artifact, rather than rewriting
   old identities or interpreting old roots as the new model.
5. No-riskgate is not a global MR-off policy. In particular, the handoff's
   USDRUBF model has MR and BO logic; future ports preserve their own semantics.
   The RI reference uses six-session warmup, not the IMOEXF four-session setting.

## Unchanged invariants and boundaries

Paper/shadow execution and ALOR comparisons remain required. They are distinct
from High180 shadow-ledger accounting. Removing that accounting does not remove
quantity/risk limits, readiness, broker-truth validation, session/holiday rules,
no-overnight checks, ownership/pending handling or durable recovery.

Preserve exact M1-to-M10 provenance and required coverage, bar availability,
candle-start model labels versus close-bound canonical identities, bounded
freshness, MR suppression before ownership, BO side-used state, ACK/request-ID
binding, replacement state seals and source XACK-last. History warmup must not
emit historical trading commands or import an ALOR position into FINAM.

Do not copy candidate09's weekend-state policy or current-close fill policy into
baseline07. The accepted FINAM paper execution policy is unchanged by this ADR.
Long offline regression/research fixtures remain useful, but their date span
does not become an operational first-boot requirement.

No Rust, Cargo, machine-consumed profile/template, installed binary, Redis or VPS
change is part of this decision record. No new O2 attempt, FINAM order POST/DELETE,
broker dispatch, runtime-live or real-order permission is issued. Retain the
terminal authority history. Review the scoped source correction, then the exact
replacement artifact; installation and bounded O2 still need their separate
operational gates. No new recovery framework is required.
