# IMOEXF V8: explicit entry, admitted seed and aggregate completion

Source candidate following accepted `9a8d1d4aa7c54e571d760523e31d84e5dc221c49`.
Implements section 8 of the independent 18.11.2026 source review. No deployment,
VPS access, real FINAM requests, native qualification or observed-session credit.

## Explicit commands

Build with `--features models-bridge` (default remains disabled). Commands:

```text
validate-ws-primary-models CONFIG REFERENCE MAPPING SEED_CALENDAR SEED MODEL_DAY RAW_CALENDAR ENTRY_PLAN MODELS_PLAN MODELS_HISTORY MODELS_CONTEXT_CALENDAR
observe-ws-primary-models  CONFIG REFERENCE MAPPING SEED_CALENDAR SEED MODEL_DAY RAW_CALENDAR ENTRY_PLAN MODELS_PLAN MODELS_HISTORY MODELS_CONTEXT_CALENDAR READ_ONLY_SECRET_FILE NEW_OUTPUT_DIRECTORY
verify-ws-primary-models CONFIG OUTPUT_DIRECTORY
```

Disabled builds recognize these names and refuse `models_feature_required`.
No implicit selection, baseline fallback, environment credential lookup, alternate
endpoint or order path. Preparation completes before credential access/output.
Observe rechecks the existing calendar window, protected credential and exact
readonly account binding. Existing auth renewal/WS lifecycle is reused unchanged.
SIGINT/TERM sets shared STOP and awaits drain/export; it does not drop the run.

This entry is **fresh only** at the accepted 07:00 freeze/start window. It rejects
committed-prefix startup, existing output and late entry. It does not cold-reset a
model in the middle of a session. A bounded observation may END mid-session;
nonflat/pending model states are preserved, not made flat to obtain PASS.

`MODELS_PLAN` is strict, duplicate-key-rejecting JSON:

```json
{
  "schema_version": 1,
  "source_config_sha256": "<WsPrimaryConfig fingerprint>",
  "generation": "<explicit bounded generation>",
  "arms": ["IMOEXF/weekday_only", "IMOEXF/weekend_context_no_trades"],
  "history_sha256": "<exact MODELS_HISTORY bytes>",
  "context_calendar_sha256": "<exact MODELS_CONTEXT_CALENDAR bytes>",
  "stop": {"kind": "bounded", "stop_at_ms": 0}
}
```

`stop_at_ms` must be after the first source close/budget and before calendar EOD.
Alternative: `{"kind":"model_eod"}`. Bounded mode sets root STOP at the declared
time, without retries, extension or overriding an earlier failure/operator STOP.
An early stop/fault remains PARTIAL. No VPS scheduler is installed by source code.

## Production seed manifest

History schema: `{"schema_version":1,"snapshots":[{"admitted_at_ms":0,
"evidence":<ClosedM1SnapshotEvidenceV1>}]}`. Context-calendar schema:
`{"schema_version":1,"required_labels_ms":[...]}`.

Inventory is an explicit pinned input, not generated from whichever bars arrived.
History must cover every requested context M10. Each original response is passed
through the existing FINAM sparse-M1 admission and exact decimal M10 aggregation.
No padding, bar-label shifts, future receipt/admission or overlapping sources.
History/canonical/source/manifest hashes, actual raw request/receive clocks and
the original history bytes are retained. No live REST proof is added to V8 WS.

The admission timestamp is the retained historical as-of declaration, checked
against collection clocks, actual now and freeze. Revalidation reconstructs the
historical admission at that time; it is not a claim that an old request is fresh
today. Numerical `durable_ms` here means declared availability at admission, NOT
proof of a collector or SQL fsync. Freeze/barrier clocks remain numerical. No
source attestation is promoted to observed readiness by this manifest.

The accepted public catalog projection is the pinned identity reference; it does
not prove current account tradability or a fresh catalog query. The manifest
contains real IMOEXF history only. USDRUBF/RIZ6 bindings initialize inactive arms
with empty history; they receive no model inputs and gain no readiness claim.
Both selected IMOEXF contexts must bootstrap ready before opening the output.

Source config, entry/calendar, model plan, manifest/initial chain and selected
arms are bound in the native control journal and exported admission. Numerical
profiles, context policies, next-open fills and strategy kernels are unchanged.

## Completion and reader

For every admitted M10: control publication -> model SQL transaction/readback ->
shared actual barrier -> native source advance -> durable advance observation.
The advance observation is after ACK, not an extra model callback. Failure to
record it cannot become completed evidence. Final writer frontier is recorded
before writer close; the actual post-join report supplies the separate drain fact.

The matching reader validates native control inputs using the existing strict
scanner, with a private **inspection-only** cohort mode. That mode creates no
restart capability. Old restart/EOD entry points still reject cohort markers.
Aggregate EOD additionally invokes the full existing source/calendar/feedback
verifier, then checks the SQL/model linkage. It is not derived from one journal.

`Journal::inspect` adds true SQLite READ_ONLY inspection while reusing the accepted
full replay/checkpoint verifier. This is the only change in finam-shadow-models;
strategy/params/context/session/arm contracts are untouched. The implementation
digest changes accordingly; old DBs are not migrated/adopted into a new run.

The reader checks exact receipts and their original serialized hashes, M1/OHLCV,
label/sequence, SQL revision, exactly two expected result IDs, ready/final states,
readback/barrier/advance ordering, actual budgets and the final source frontier.
Complete raw recovery GETs must be present in control JSONL and match references.
It rejects missing arms, mismatched frontier/receipt/manifest and missing raw proof.
Input/DB changes during inspection are refused. Inspection does not rewrite DBs.

Statuses:

- `BOUNDED_COMPLETE`: coherent source/SQL/control prefix and clean bounded drain
  at the planned STOP, without requiring artificially flat positions.
- `MODEL_EOD`: complete 101-bar grid, both models complete/flat, full native control
  EOD verification and matching actual source frontier/drain.
- `PARTIAL`: retained durable result, but no completed cohort claim (including
  SQL committed before source ACK, early STOP or a fault).

Damaged/unverifiable or undrained output gets `models-incomplete.json` and no final
cohort manifest. Undrained writers' mutable files are never sealed. Evidence and
model state are retained. A create-new cohort manifest is published last after
fsync; only this attempt's marker is revoked if final publication fails.

Export retains `journal.jsonl`, `models.sqlite`, raw history/calendar, exact plan,
derived manifest, admission, post-join report and aggregate completion. Both
journals are required. Verification returns exit 0 only for completed statuses;
PARTIAL is exit 2. A manifest alone is not a process/drain/durability witness.
`observed_clean_session`, full-session/network/cgroup/parity qualification and
broker execution remain false even when model EOD is coherent.

## Checks and next boundary

Targeted tests cover sparse source-derived seeds/readiness, bad bindings/future
inputs, late/stopped/colliding output; real managed writer/continuity-gate
live -> WS historical -> live and live -> REST historical -> live; STOP after SQL;
full model EOD; read-only DB checks; missing arm/source/frontier negatives; CLI
feature-off/refusal behavior. Numerical regressions run separately.

Managed fixtures are SYNTHETIC compressed days, not provider/native/soak evidence.
The peer answers real Ping throughout long SQL tests. Test clocks advance only
after model source advance, not the earlier baseline commit. No production
timeouts were enlarged to fix the fixture. A source review package retains exact
fixture bytes; final test counts/toolchain/logs are in its evidence manifest.

Next after SOURCE ACCEPT: exact-source Linux runner AND matching reader build,
native qualification and real latency spans under unchanged LIVE/STOP budgets,
including post-join export/inspection duration. Only then an execution package
and separately authorized bounded launch. Automatic cohort restart, intraday
cold start, USDRUBF/RI activation, VPS/P0/O3/DB0 changes and live orders stay closed.
