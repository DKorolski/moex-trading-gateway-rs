# USD/RI executable/input glue — source-only R1 candidate

Accepted parent: `545b832f8e938d00913b93ef4830c039673a832e` (managed
V8 R1, MGR-1 closed). This slice exposes the existing managed source through
explicit commands. It is not a Linux release, installation or launch approval.

R1 corrects reviewed R0 `1a843321`; see
`usdrubf-ri-executable-input-glue-r1-20261119.md`.

## Scope and fixed bindings

Two independent existing V8 coordinators/calendars/physical subscriptions and
one serialized numerical SQLite writer. SQL readback/source-advance-last order,
transport, auth lifecycle and managed reader are retained. R1 adds an atomic
continuity/native-advance fence; no SQL, log IO or model callback is inside it.

USDRUBF@RTSX: asset 3447194, tick 0.01, trade lot 1; the reference/contract
size 1000 is not the order lot. RIZ6@RTSX: asset 5436517, tick 10, lot 1,
expiry 17-Dec-2026, no automatic rollover. Current metadata, exact selected
account, raw independent schedules and frozen catalog must agree.

Four USD/RI arms are admitted READY with zero new model bars. IMOEXF arms
remain unready and receive no inputs. Model kernels, parameters, feedback,
fees and close-fill rules are not changed. No broker order route exists here.

## Private input and commands

Build with `cargo build --locked -p finam-signal-shadow --features models-bridge`.
Without that feature all four commands return `models_feature_required`.

- `prepare-usd-ri-managed DRAFT NEW_FROZEN_INPUT`: file-only preparation;
  create-new mode 0600, fsync file and parent. Output status is
  INPUTS_FROZEN_ONLY, **not** fresh admission.
- `validate-usd-ri-managed FROZEN_INPUT EXPECTED_SHA256`: verify exact file
  bytes against the independent prepare receipt, then derive history and plan again,
  compare exact pins, then run existing real-clock metadata/calendar/model
  admission; no secret, output directory or network.
- `observe-usd-ri-managed FROZEN_INPUT EXPECTED_SHA256 READ_ONLY_SECRET_FILE NEW_OUTPUT_DIRECTORY`:
  fresh entry admission, exact-account shared AuthLifecycle, two fixed GET-only
  clients, existing managed V8 run. This is implemented source, not permission
  to call FINAM or launch on the stand.
- `verify-usd-ri-executable OUTPUT_DIRECTORY`: read-only entry closure plus
  the accepted causal managed reader. The old `verify-usd-ri-managed`
  remains available for library-only artifacts.

Draft schema `finam.shadow.usd-ri.input-draft.v1` has account_id, generation,
session_date, start_not_before_ms/start_before_ms/stop_at_ms and exactly two
sources. Each source has instrument, original asset/params ScheduleSnapshot,
RawCalendar, bootstrap_reference_start_ms and history. History has the exact
instrument, required_labels_ms and snapshots {admitted_at_ms, evidence}, where
evidence is the existing ClosedM1SnapshotEvidenceV1 (full raw REST parts,
GET endpoint/range/status/completeness/body length/hash and original clocks).
The retained synthetic draft in the review package is a concrete schema example,
not deployable inputs. Real account-bearing draft/frozen input remains private.

Preparation rejects duplicate/unknown JSON keys, oversized or non-regular
input, mismatched/duplicate instruments, overlapping snapshots, future original
clocks, invalid endpoints/ranges/hash/OHLCV, uncovered or empty M10. Each M10
uses actual admitted M1 only; no gap filling or timestamp shifts. Calendar and
model bootstrap validate required complete context separately; sparse M1 is
not itself a missing M10. Original receive/admission times are preserved.
Historical seed is DECLARED_CLOCK_REPLAY; it is not a physical historical
fsync or observed live availability claim.

R1 requires RawCalendar coverage from the first query's local date through the
cutoff. Every historical trading/auction interval must be covered by a complete
REST request, including intervening weekends. The inventory must equal ALL
nonempty M10 buckets in the admitted M1; no subset selection. Actually empty
buckets remain absent and model anchor thresholds are unchanged. Unknown
nested M1/decimal fields are refused before public raw export.

EXPECTED_SHA256 comes from the independently retained prepare/capture workflow,
not from hashing the current input immediately before admission. Replacing input
and its expected value together is not an independent pin. This protects
integrity after preparation, not upstream authenticity; original capture clocks
and full receipts still need a trusted collection procedure.

Frozen schema `finam.shadow.usd-ri.executable-input.v1` contains the unchanged
draft plus derived plan/manifest. There is no supplied-manifest bypass, URL
override, clock override, resume or fallback to an IMOEXF runtime. Source entry
remains the accepted fresh-first-session 07:00–07:09 MSK gate; changing a timer
cannot authorize an intraday entry.

## Secrets, STOP and artifacts

Reuse existing FINAM readonly/account/permission/actual-expiry checks. A
private credential file is accepted with the existing rules; group-readable
credentials require the exact read-only systemd mount
`/run/credentials/usd-ri-shadow.service/finam-read-only-secret`, root ownership
and pinned directory/file modes. Existing IMOEXF/diagnostic namespaces do not
cross-authorize it. Errors printed by CLI are static codes; no account, token
or raw credential is printed.

SIGINT/SIGTERM is registered before starting work. Signal sets the shared STOP
and awaits the same managed future (does not abandon its workers). Independent
external STOP/cgroup qualification still belongs to the next Linux release.

New public artifacts: entry.json (input/account hashes and public admission),
entry-history.json (original market-data seed evidence, no params account),
entry-result.json. Existing model-manifest.json, models.sqlite and
managed-control.jsonl remain authoritative. Final result is created only after
both worker reports show finalized/drained and the matching reader validates
SQL/control/source-last and original seed reconstruction. Final file and
directory are fsynced. Export and reader use the same finalized/joined predicate.
A repeated failed export cannot poison an existing final with another marker.
Missing/unjoined/invalid completion yields entry-incomplete.json / INCOMPLETE,
not a clean result. HELD stays HELD (CLI exit 2); malformed/incomplete or
tampered readback refuses (exit 1). Observe INCOMPLETE is exit 1 too.
Verified synthetic source prefix is exit 0,
but observed_clean_session/native_systemd_qualified/operational_authority/orders
are always false in this source-only slice.

## Qualification boundary / next step

Local synthetic physical WS streams exercise both existing V8 drivers,
one-lane reconnect/resubscribe, shared SQL, STOP/join and matching CLI readback.
Stale historical fixtures must refuse actual executable admission. No fake
production clock is added to make fixture admission pass.

After source acceptance: exact Linux/amd64 release and isolated native
systemd UID/credential/STOP/join/fsync timing qualification, then a separately
accepted bounded execution procedure. No VPS, FINAM, real key, Redis/DB0,
installed IMOEXF, timers, GitHub or origin/main change is part of this slice.
