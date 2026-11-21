# Model-free auth/WS diagnostic — source candidate

## Structured error correction (26 October, source candidate)

Correction baseline: cf2fb58af2865e9feaecb6c7fc5bcd7a82748f25.
The 16:00 attempt is accepted FAILED evidence, not auth/WS qualification.
Its original root cause cannot be recovered; this change does not claim to fix
an assumed FINAM payload format.

Socket failures now retain a finite stage/code, including lease, request/connect,
EOF/read/close/unexpected-message, protocol receive, subscription build/send/commit,
pong flush and silence. Existing WsError variants and QuoteError variants survive;
parser-owned Frame branches use fixed FrameCode values. Dynamic provider, mapper,
HTTP and tungstenite error payloads are discarded, never formatted into evidence.
Budget/Stopped/Deadline/Held/Clock errors retain their existing transport class.

Diagnostic worker errors attach channel, auth_generation and connection_generation.
A connection not established yet is null, including a failed rotation connection.
The terminal primary_error carries this structured context; non-WS failures retain
their previous fixed reason and may have null primary_error. The first coordinator
failure is not overwritten by sibling cancellation, drain failure or deadline
cleanup. Final-flush rejection remains unchanged and cannot qualify a run.

Local duplex tests exercise malformed protocol/provider/validation failures in
both channels through the actual Session and terminal coordinator, with two jobs
joined and FAILED/exit1. Sentinel payloads are absent from the recorded JSONL.
EOF/read conversion and actual Close reception are separately checked; existing
transport and successful/incomplete/stop/final-flush tests remain required.
These are local fixtures, not operational rotation or graceful-stop evidence.

No strategy, admission predicate, auth policy, retry, reconnect, output budget,
stop budget or timing gate is relaxed. Source review is required before release
qualification, installation or a separately authorized new run.

## Original diagnostic contract

Accepted predecessor: `6f1fb5df187051739e5a4e26d71270a7cc46da81`.
Scope: reviewer AUTH/WS DIAGNOSTIC scope acceptance dated 26 October 2026.
No source/release/operational acceptance of this new mode is claimed.

`auth-ws-diagnostic PLAN READ_ONLY_SECRET_FILE NEW_OUTPUT` is separate from
`observe`/`observe-prefix`. It uses no model, seed, feedback callback, historical
prefix, model REST worker, Redis or trading route. Compilation dependencies are
unchanged. Existing production credential binding and observation flow remain.

One shared AuthLifecycle serves two independently owned BarsSocket/QuotesSocket
workers. Existing single-flight grant validation, actual expiry/renewal, consuming
rotate_auth, retained protocol/dedup, silence/freshness and S5 auth witnesses are
reused. Controlled S5 reconnect is not invoked. ReadOnlyCollector siblings share
a stop latch; watch cancellation additionally covers pending grant/connect/send.

Plan has exact private account binding, fixed IMOEXF@RTSX, schema1/run identity,
start/latest-start/finish timestamps. Latest start is at most 30 seconds after
start; total window at most25 minutes. Fixed budgets: request15000ms,
silence60000ms, reconnect5000ms, quote age5000ms, dedup4096, stop1000ms;
output16384..1048576 bytes. Unknown fields, late/early starts and changed budgets
are refused. Start receipts carry a canonical private-plan digest, not account.
Deployment evidence must separately bind the exact private plan bytes/hash.

The diagnostic CLI alone selects the additional fixed credential pair:
`/run/credentials/signal-shadow-auth-ws-diagnostic.service/finam-read-only-secret`.
Environment alone does not authorize a path. Exact root:root0440 regular file,
root:root0550 directory, canonical path and unique readonly tmpfs/ramfs mount
remain required for that systemd case. Original regular private-file rule remains.
Production observe continues to select only signal-shadow.service.

Evidence is allowlisted: verified grants with generation/expiry and binding
booleans, per-socket witness with auth/physical generations, subscription send
and first nonempty validated DATA times, terminal owner/join count and reason.
No raw frames, provider errors, account identifiers, credentials or their hashes
are written. File is create-new0600, bounded, synchronously flushed. Evidence
failure cannot return PASS.

PASS/exit0 requires both channels' generation1 and common generation2 evidence,
advancing expiry, DATA after subscription on each owning socket, both owners
active before expected stop, two joined jobs and completion before deadline.
Expected SIGINT receipt window is finish minus20s through finish minus5s, aligned
with external timer at minus20s and15s systemd stop timeout. Early interruption
or missing rotation/DATA yields INCOMPLETE/exit2. Worker/protocol/auth error,
missing external guard at deadline, join timeout or overrun yields FAILED/exit1.
Abort after join timeout is cleanup only, never graceful success. No auto-retry.

R1 final-flush correction: the durable success candidate is explicitly
`DIAGNOSTIC_PENDING_EXIT`, never an unconditional `DIAGNOSTIC_PASS`.
`requires_successful_process_exit=true` requires independently captured normal
process exit0; missing exit evidence, crash, signal termination or exit1 cannot
qualify. After the last mandatory write_all/sync_data, BOTH wall and monotonic
clocks are checked again. Equality with or passage beyond either deadline returns
`diagnostic_final_flush_deadline` (CLI FAILED/exit1), overriding pending evidence
without relying on another potentially failing write. `record_prepared_at` is
the pre-flush record preparation time, NOT completion. Actual process/cgroup
completion before finish must still be established by external native/run evidence.
An output-only reader MUST NOT interpret the pending record as PASS.

Duration limits use exact chrono durations (including subsecond precision), not
truncated whole seconds. Private final-clock tests simulate final flush crossing
the deadline on each clock independently, with complete coverage, expected stop
and both jobs joined; the no-overrun control retains exit0 qualification.

Source tests cover coordinator synthetic event schedules, cancellation/drop of
pending futures, sibling failure, missing/old generation and duplicate-only
witness refusal, output limits, timing/budget checks and credential path separation.
These are deterministic composition tests, not real FINAM rotation or new native
credential qualification. Existing auth/socket tests remain regression evidence.

Next gates: source review; Linux release/native dummy qualification for diagnostic
path; separately reviewed exact unit/plan/stop guard; separately authorized run.
Installed production 6f1fb5df, P0/O3 and credentials are untouched. No14:10 timer
or other operational window has been installed. Full model shadow/parity/EOD and
live micro remain outside this diagnostic's claims.
