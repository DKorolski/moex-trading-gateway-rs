# O3 local source implementation — source review candidate

2026-10-08. Parent preparation `bc9813434b35238e9e6e861476cab2aaf3fc72a2`
has PREPARATION ACCEPT. This source delta is **not** artifact acceptance or
operational authority. Local production-process
qualification passed on 2026-10-09 (run7, below); it is not VPS acceptance.
O2 remains accepted; operational O3 remains NOT_READY. No additional preparation
R1 review is required for the matrix clarifications.

## Implemented local components

- `Stage8bP1fAuthorityStoreV1::claim_o3_phase` rejects O1/O2/O4 before writes,
  then reuses the existing signed claim protocol. It never publishes an O2
  selector, creates genesis, resets generation or erases terminal history.
- An immutable Published-predecessor checkpoint is retained before advancing
  the observed V2 producer high-water. Restart reconstructs the two-entry
  Published window from explicit paths, independently supplied receipt bindings
  and exact expected canonical bytes. Missing/corrupt/foreign/unpublished
  states fail closed. No directory discovery or Redis-derived authority.
- The existing fixture-linked paper lifecycle test now drops the original
  Published window, restores it from disk and feeds that restored value into
  the existing callback/outcome/truth/XACK/restart harness. It is still a
  historical local regression with fixture signing, **not** the requested
  production-executable witness.
- A bounded AF_UNIX schedule signer adapter carries a full canonical unsigned
  envelope. Production service construction pins the accepted generation-2
  public key. Policy pins identity, config/instrument/registry, semantic digest
  and a maximum 1800-second interval. Arbitrary digest signing is rejected;
  ordinary schedule verification precedes signature release. Connected peer
  UIDs, frame size and read duration are checked. Unit tests use a private
  test-only constructor and explicitly prove production rejects their key.
- The O3 matrix now labels inherited/partial local frontiers separately from
  unexecuted operational frontiers. O3-12 references the existing CANCEL
  transport-response-loss test, not an ordinary successful Market XACK.
- Added the fixed `guardian-claim-o3-fixed` command; it uses the existing
  authority public key and O3-only claim, without the O2 selector. The local
  guardian selects the new O3 child from its admitted permit and retains the
  existing deadline, process-group handling and restart budget.
- Added `stage8b-p1f-o3-composition run PHASE_SHA256`. It verifies the signed
  phase, root-owned fixed composition/config files and independent snapshot,
  receipt and canonical-M10 pins. It waits on the real clock for the two
  prescribed intervals, uses existing fixed DB15 producers, retains/reloads
  the Published pair, and calls `execute_stage8b_p1e_observed_run_v1` directly.
  Namespace initialization, bootstrap and FINAM transport are not exposed.
- Added `stage8b-p1f-o3-signer run PHASE_SHA256`: separate Linux root process,
  fixed credential/socket paths, generation-2 public-key check, active-history
  checks, wall/monotonic deadline, peer UID, bounded full-envelope requests,
  zeroizing credential buffers and no core dumps. This is source code, not an
  installed or operationally qualified service.

The schedule envelope format is unchanged: source generation **1**, signing
key generation **2**; observed M10 remains canonical V2/source generation **2**.

## Source review boundary (2026-10-09)

The [handoff guide](stage8b-p1f-o3-source-handoff.md) fixes executable/input/
credential paths and future execution order. Disabled service templates are
packaged separately from the local witness. The source handoff verifier binds
both retained source snapshots, run7 ELF identities and exact logs to the
immutable commit. This is the single source review boundary, not a new VPS
bootstrap, root replacement, key rotation or installation authorization.
Current-tree authority still describes the accepted parent; source acceptance
and governance closure remain review/follow-up work, not claimed here.

## Reproducible local component checks

Run `python3 -B scripts/stage8b_p1f_o3_local_components.py NEW_OUTPUT_DIRECTORY`
with the normal cached `CARGO_TARGET_DIR`. It records 23 exact local tests,
three O3 binary builds without test features,
fmt, strict clippy for both affected crates/all features/all targets, diff
hygiene, source-byte stability and the matrix distinction. Redis instances
belong to the isolated existing test harnesses. Output explicitly records
`production_executable_linked_proof=false`, `production_schedule_key_used=false`
and `operational_o3_ready=false`. It is not the complete O3 acceptance gate.

The 2026-10-08 `components-run3` run passed all fourteen exact selectors,
both default-production binary builds, fmt, strict clippy and diff hygiene.
One input regression caught the guardian/schedule canonicalization difference:
guardian V1 uses struct-order serde bytes; schedule and O3 plan use sorted
canonical JSON. The child now preserves the accepted guardian signing bytes.

Linux/amd64 debug builds also completed with `--offline --locked` and no test
features, using Rust 1.95.0. The offline cache was first populated from public
Cargo registries; compilation and executable checks then ran without networking.
Four actual-binary entry negatives passed (missing arguments, root paper child,
missing fixed config and non-root signer). No key/config/Redis was mounted in
these negative containers. The three no-default-feature dead-code warnings
are in the existing Stage8A1 capability module, not the new O3 code.
This is component ELF/build evidence, **not** the positive paper lifecycle proof.

## Local credential custody check

The operator authorized use of the existing generation-2 schedule credential
solely in an isolated local signer, not VPS deployment or operational O3.
The original generation-2 ceremony was found outside the repository. Its public
manifest matched the accepted generation-2 manifest byte-for-byte. A separate
local `public-key` process derived exactly
`432a274889bc2b3a4492b3a1510f7356e1c2878337263e28472335f8600e4d99`;
directory/key modes were 0700/0600. At that fingerprint-only checkpoint no
private bytes had been printed/copied and no signature issued. The later
isolated executable witness below used the selected credential under explicit
local permission; it still never embedded it in an image or sent it to the VPS.
The custody path is intentionally not recorded in a review artifact.

## Production signer executable witness (local only)

The subsequent Linux/amd64 `signer-linux-run7` completed with
`PASS_SIGNER_ONLY`: the ordinary publisher requested an actual signature through
the fixed AF_UNIX socket and the unchanged production verifier accepted it.
The existing generation-2 schedule credential was used; the schedule trust
key was not substituted. These preliminary runs used the
real clock, then restarted the signer against the same local phase. Both
SIGTERM stops returned exit 0 and removed the owned socket. Foreign identity,
stale envelope, raw digest, oversized frame and a connected root peer were
rejected. The disposable network-none container was removed.

Build-evidence correction: runs 7/8 built the Cargo example and signer together.
That enables the example's dev-dependency fixture features even with
`--no-default-features`; those runs cannot prove a production-feature signer.
The harness now builds the example first and the signer in a separate Cargo
invocation, records compiler-artifact feature lists, and fails if any of the
three relevant production libraries enable anything beyond the empty default.
The local example remains explicitly a fixture client, not production ELF.
The corrected signer-only `signer-linux-run9` passed with separate builds:
the three relevant production libraries enabled only their empty default
features; signer target features were empty. The fixture client is recorded
separately. Both signer stops returned 0, authority history was unchanged,
and the disposable container was removed.

## Fresh DB15 administrative composition (local source only)

The accepted fixed initializer creates only M10/command streams and groups.
The ordinary runtime additionally requires the exact deployment manifest and
eight output streams; the schedule publisher requires a pre-existing schedule
stream. This is an actual composition gap, not a reason to relax verify-only
runtime or to reset any existing Redis state.

The new separate `stage8b-p1f-o3-provisioner initialize-fixed PHASE_SHA256
INITIALIZE-FRESH-O3-DB15` is root-only and accepts only the signed fixed O3 plan,
current Active phase, config-bound manifest and authenticated generation-2
first schedule envelope. It reuses the existing M10/command initializer;
non-consumed output streams get one administrative marker. The unchanged
schedule reader validates every row, so its stream MUST NOT get a placeholder.
Instead the initial row is the exact signed sequence-1/revision-1 envelope.
The publisher retains Prepared and may replay those same bytes before normal
sequence continuation (existing exact-duplicate semantics are unchanged).

This is an explicit narrow source extension of the administrative role: first
`XADD` creating the schedule stream with verified payload. The accepted runtime
publisher still uses `NOMKSTREAM`; no delete, trim, consumer rewind, clear,
repair, runtime initializer or new Lua engine is added. Manifest serialization
shares the existing validator, rather than duplicating its acceptance rules.
Any pre-existing/partial inventory is rejected before writes; a successful
run is also rejected on a second invocation. An interrupted initialization
remains quarantined, not automatically repaired. The command is NOT installed
on the VPS or authorized there; review must include this source extension.

Three local Redis regressions cover exact inventory/groups, repeated/partial
initialization rejection, and invalid manifest/signature before connection.
The Linux witness also exercises the separately built production provisioner
against disposable loopback-only Redis with a real generation-2 signature;
its result remains explicitly separate from full paper lifecycle proof.
The first such `signer-redis-linux-run10` passed: 12 exact DB15 keys, two empty
input streams with accepted groups and zero PEL, exact authenticated initial
schedule, repeat rejected without changing inventory or schedule. Both local
containers were removed. `components-run6` passed all 18 selectors, three
binary builds, fmt, strict clippy and diff checks.

The fixture client additionally checks ordinary Prepared replay (same exact
signed bytes), next publication sequence 2/revision 1, and strict Redis reader
acceptance of the duplicate plus retained high-water readmission. This is a
publisher/reader subchain, NOT a Hybrid paper outcome or durable-owner restart.

This found and corrected a real integration defect: a newly constructed
synthetic Stage4 report used nanoseconds while the accepted schedule wire used
microseconds. New O3 synthetic observations now sample the real clock at wire
precision BEFORE constructing the report. Existing broker receipts/reports
are not rewritten, and the verifier is unchanged. A sub-microsecond regression
checks exact equality of report and declared observation timestamps. Guardian
timestamps remain the separate accepted whole-second UTC format.

The selected credential was mounted read-only behind a native root-only
directory, staged only in a root-only tmpfs credential, and never included in
an image or copied to the VPS/Git/evidence. A negative Linux permission check
showed why a bare Docker Desktop bind mount is insufficient: host permission
virtualization alone allowed `test -r` from the paper UID. The native 0700
ancestor plus tmpfs custody now rejects access to BOTH paths. No private bytes
were printed; evidence contains only public envelopes, fingerprints and logs.
The new credential unit regression covers wrong owner/mode, hardlink, symlink,
FIFO, malformed/oversized bytes and exact binary/hex representations.

Reproduce explicitly with `scripts/stage8b_p1f_o3_signer_linux_witness.py` after
separate local key-use authorization. Its example fixture creates only an
ephemeral container-local guardian authority; it is NOT an O3 claim on the
VPS, not a trust rebind, not a durable root, and not an initializer. Future M10
pins in this signer-only fixture are offline input calculations: no M10 is
published and no runtime is started. Synthetic schedule sessions in this
credential witness are not evidence that the strategy may trade at that time.

**Still false:** production-executable linked paper proof, paper outcome,
runtime restart/high-water proof and operational O3 readiness. A normal
no-riskgate paper lifecycle must still run in an eligible model-session
window. Do not relax model hours or substitute a historical clock to get a
positive outcome. Finish the local Redis/root fixture and combined executable
runner before issuing the single complete source-review package.

No VPS command, DB0 contact, installed-root mutation, O2 replay, operational phase issuance,
FINAM endpoint call, GitHub workflow, push or merge is part of this checkpoint.
Do not create a review ZIP claiming a runnable O3 until the remaining linked
production path has been implemented and qualified.

## 2026-10-09: first schedule checkpoint integration correction

`signer-redis-linux-run11` and `components-run7` passed the previous subchain
(18 exact selectors). They still did not launch the full Hybrid runtime.
Further composition inspection found an unqualified first-publication edge:
the initializer's signed sequence-1 Redis row and the runtime's initial state
could be prepared separately. If the actual publisher checkpoint was missing,
the loop could sign a DIFFERENT sequence-1 envelope instead of exact replay.

The correction keeps first preparation separate from runtime execution:

1. The Linux/paper-UID command `stage8b-p1f-o3-composition
   prepare-initial-schedule PHASE_SHA256 PREPARE-FRESH-O3-SCHEDULE` checks the
   fixed signed inputs, holds the producer lock and requires an otherwise
   empty producer directory. It uses the constrained signer, persists and
   rereads the ordinary Prepared checkpoint, then emits only its public exact
   envelope. No Redis/bootstrap/runtime call is made. Repetition is rejected.
2. Before any Redis connection the root initializer requires that fixed
   paper-owned checkpoint to be canonical, production-authenticated,
   Prepared/sequence-1/revision-1, plan-bound and byte-identical to the protected
   `initial-schedule.json`. Missing/foreign/advanced/different state fails closed.
3. The runtime schedule loop has NO implicit First branch. It must load the
   bounded, custody-checked checkpoint, replay Prepared as-is, or resume from
   Published. Missing state cannot generate a replacement signature/sequence.

Checkpoint reads reject links, extra hardlinks, FIFO/device input, wrong owner,
permissive modes, oversized/changing bytes and non-private parent directories.
The ordinary publisher now creates its replacement state file as 0600 on Unix,
independently of the caller's umask. Signature/wire semantics are unchanged.
The Linux witness now builds the production composition binary separately from
the fixture example as well. It tests missing-checkpoint runtime/provisioning
refusal, distinct valid signed initial bytes, repeated preparation rejection,
exact initial-row replay and reader high-water continuation. Those checks still
do not constitute a production paper-outcome witness.

Combined execution also exposed that the existing authority `inspect` uses an
exclusive nonblocking guardian lock. The O3 signer and initializer now wait at
most 250 ms ONLY for `ConcurrentGuardian`, without serving/signing while the
head is unverified. They require a new successful exact-Active inspection;
other errors, foreign/terminal heads and timeout remain failures. Phase expiry
is checked before/after; this never resets or extends a phase. A regression
covers transient contention, terminal state after contention, custody errors
and bounded failure. The local fixture also installs root-owned PUBLIC inputs
with the paper read group; private signing credentials remain root-only.

### Local full-witness history choice, not an O2 migration

At 2026-10-09 after 04:00 UTC, the accepted O2 seed beginning October 2 04:00 UTC
is outside the seven-day window for a NEW realtime bootstrap. Existing-root
authenticated historical recovery is a different path and is not invalidated
by that age. No VPS root/key/history or timestamp was changed to work around it.
The audit ZIP does not supply the private HMAC needed to clone that root.

On 2026-10-09 the operator explicitly authorized the isolated executable test
to use its own synthetic recent-session history and disposable root, while
keeping the accepted O2 source/root byte-for-byte untouched. This authorizes
LOCAL fixture inputs and an ordinary production bootstrap, not an operational
installation or an O2 migration. The linked proof is still pending execution.
Do not broaden the seven-day limit, reset the accepted root, move timestamps,
substitute a historical clock, or use a fixture signer to bypass this boundary.

The `--linked-runtime` witness builds five production executables separately
from the input-generation example. It uses real wall time and the unchanged
V2 profile. A first current-session bar closing 07:10 Moscow plus the accepted
three-hour BO wait makes 10:10 the earliest decision; the following M10 must
also actually close before the runtime consumes the typed Published pair.
The local fixture helper creates no durable root or intents. The production
bootstrap alone creates the disposable authenticated root, using an ephemeral
local HMAC credential in tmpfs. The existing generation-2 schedule credential
remains root-only in the constrained signer; no private data enters evidence.
The witness requires actual paper trade/ACK/position and a completed journal
request, empty source/command PELs, supervised child SIGKILL/restart without
duplicate outcomes or M10 high-water change, and bounded parent stop. Failures
retain public diagnostic evidence and dispose only the witness containers.

## 2026-10-09: full-process run5 exposes post-Market continuation gap

Status: **LOCAL LINKED PROOF HOLD**. Not SOURCE/ARTIFACT/OPERATIONAL ACCEPT.
No O3 review ZIP or operational authorization is produced at this boundary.
The 13 operational matrix rows remain NOT_RUN. The accepted O2/VPS root,
installation, first-boot history and generation-2 ceremony are unchanged.

Local evidence lives under the project `reports/operations/` directory:
`o3-implementation-20261008/production-runtime-linux-run5/`.
Five ordinary production executables were built without fixture features;
the separate fixture builder supplied synthetic recent history, not runtime
authority. The process used real time, local DB15, a disposable root and the
constrained existing generation-2 signer. No network broker transport existed.

Observed, not inferred from a test label:

- Decision M10 `1791536400000-0` (09:00 UTC) and execution successor
  `1791537000000-0` (09:10 UTC) were published.
- The hash-checked Stage 6 journal contains six frames: RequestAccepted,
  ScheduleEvidenceBound, DispatchAttemptRecorded, BrokerOrderObserved,
  BrokerTradeObserved and RequestFinalized(Completed), each once.
- The terminal paper result reached recovery seal generation 5 and the
  runtime Redis audit records a successful `source-xack-last` for the
  original decision. Initial first-boot transaction/receipt bytes did not
  change between bootstrap and the retained failure snapshot.
- On the next semantic acquisition the child returned
  `RestartRuntimeRequired` before another callback. This is not a signer,
  FINAM, VPS, historical-data-density or GitHub failure.
- After preserving the observed failure, the local parent was sent SIGTERM.
  The wrapper then reported signer exit during shutdown and FAIL. That final
  wrapper error is not the initiating lifecycle failure. Both disposable
  containers were removed; no local test services remain running.

Exact cause: `Stage8bP1d2TruthCommittedOwner::into_ready_after_source_resolution`
returns the generic Ready owner without converting its embedded authenticated
P1-d2 truth lifecycle. `commit_stage8b_p1_semantic` routes that owner to
`apply_stage8b_p1_semantic_transition`, which admits only P1SemanticReady.
The P1-d3 terminal route does not apply: no P1-d3 replacement is present.
Merely invoking the existing migration is not a solution: Migrated is not
terminal semantic-continuation authority either.

The new exact local test
`stage8b_p1e_process::tests::o3_post_market_successor_reproduces_missing_continuation_authority`
reproduces the same missing transition using a real isolated Redis and the
ordinary acquisition/latch/resume functions. It proves the rejection leaves
the successor in PEL, does not change durable files or command/ACK rows and
does not call Hybrid/provider/publication/XACK again. **Its PASS characterizes
an OPEN blocker; it is not successful semantic continuation.** The previous
`bounded_signed_market_cycle_rejoins_ready_and_xacks_source_last` test stopped
before this acquisition, as did the earlier fixture-linked restart proof.

Validation before that diagnostic was added: component run14 passed 22 exact
selectors plus fmt, production build, strict clippy and diff checks. The
process-module suite passed 71 tests with 7 ignored child helpers. These are
local regression results, not full-process O3 acceptance.

Further implementation needs one explicit, reviewable post-Market continuation
delta: after exact S_truth/source resolution, preserve the authenticated
order/position state and callback watermarks; admit only a strictly newer
exact M10; persist/reread the resulting replacement before its XACK; prove
restart at both sides without duplicate truth callbacks or effects. Do not
simply broaden the lifecycle enum allowlist, migrate to an empty working book,
skip the successor, inject position state, or stop the witness early to claim
the existing continuous-runtime contract. P1-d2 truth currently records the
canonical truth state separately from applying its order/position callbacks
to Hybrid; that distinction must be handled explicitly in the narrow delta.

The witness also must measure the accepted paper-result channel correctly:
the finalized outcome is in the authenticated journal/replacement, whereas
the external ACK/trade producer streams in this run contain only initializer
rows. Their lengths must not be presented as actual paper outcomes. Fix the
qualification assertion against exact retained journal/seal evidence when
implementing the continuation, not by creating synthetic Redis outcome rows.

Retained run5 diagnostic hashes:

| File | SHA-256 |
| --- | --- |
| local-supervisor.log | 73f8d953dc1bcfaca6798c0d70d31eb8f094e1d14bc33b518b35c8b95e06eeb2 |
| root-at-failure/stage6.journal | 6aabd278f0cffa6b70243e76917d1165b59b1f9df79bb683b26b0d548a81c5e7 |
| root-at-failure/stage7b-recovery.seal | 09d168722d1c1901a62f82d94e176cf38c522e1830931aa73e206dfdce86b3fb |
| result.json | 0ea43a2da5fd2c807eb351e2a18f211015c2ae46cf913e96d8a06a848eb94e0b |

### Authorized post-Market continuation correction (local, 2026-10-09)

The operator authorized the narrow lifecycle delta after the run5 blocker.
The historical diagnostic above remains evidence of the original failure;
its test is now a positive regression named
`o3_post_market_successor_preserves_position_and_commits_before_xack`.

The correction consumes exact authenticated P1-d2 S_truth only from the
source-resolved owner, validates a strictly newer M10, applies the existing
order/position callback bridge (which rejects emitted intents), and executes
the ordinary next semantic callback. The intermediate migrated package is
memory-only: only the final semantic replacement is persisted/reread before
source XACK. Existing P1-d3/P1-d4 formats and sequence history are retained;
no position injection or new persistence format is introduced.

A bare Migrated package still has no fresh-bar permission. A Migrated package
with a validated committed semantic watermark can continue strictly forward;
restart first resolves that exact committed source, without repeating the
truth or semantic callbacks. The positive local Redis regression verifies
canonical order/trade/position state, Hybrid position, unchanged journal and
command rows, already-XACKed restart with no durable mutation, and another
fresh zero-intent M10 after restart. Legacy migration behavior is unchanged.

The full executable witness now checks the actual authenticated paper-result
channel (journal plus embedded order/position state and Hybrid position),
not fabricated external producer rows. Full-process requalification and
broader regressions are pending. All 13 operational O3 rows remain NOT_RUN;
VPS, accepted O2 installation/history and live surfaces remain unchanged.

Run6 reached one terminal paper Market, replacement seal generation 6 and
semantic watermark 1791540000000-0, with both source M10 entries resolved.
It did not qualify the complete witness: the observer incorrectly required
command-group lag=0, although this path publishes a command and uses its typed
paper owner without starting an independent command-stream consumer. The
correct assertion is untouched command group (last-delivered=0-0, pending=0,
lag=1) plus exactly one completed journal lifecycle and the resolved M10 group.

Stopping that diagnostic exposed a composition race: the guardian entered
Stopping, the signer correctly refused further authority, and the publisher
failure won against the runtime's graceful signal handling. The composition
now observes external SIGTERM/SIGINT itself, stops the schedule task and awaits
the ordinary runtime drain. An unprompted publisher exit remains fatal. Run6
is retained as FAIL, not relabelled; clean process/restart/stop requalification
is required after these narrow observer/signal corrections.

### Clean local production qualification: run7 (2026-10-09)

Result: **PASS_LOCAL_PRODUCTION_PAPER_LIFECYCLE**. Evidence directory relative
to the main project: `reports/operations/o3-implementation-20261008/production-runtime-linux-run7`.
Its 125 retained evidence-file hashes were independently reread locally with
zero mismatches. Source bytes were unchanged throughout the executable run.

- Ordinary production bootstrap; recent explicitly synthetic four-session
  inputs; generation-2 constrained signer; real wall clock, no test-feature
  runtime libraries or injected intents.
- Real closed decision/successor M10 at 10:10/10:20 UTC. Exactly one completed
  paper lifecycle in six authenticated journal frames (including the schedule
  binding), one order/trade/position, ACK/truth sequence 1/2, and Hybrid position
  +1 after semantic continuation. Both exact M10 source entries resolved.
- SIGKILL of the exact child PID 409; ordinary supervisor restarted PID 532.
  Journal, replacement seal, producer high-water and captured Redis source/
  command/truth rows remained byte-identical. First-boot marker/receipt unchanged.
  The reread runtime reported healthy/paper_ready.
- External SIGTERM: parent exit 0, GRACEFUL_STOP, two child starts, no forced
  kill, stop 0.529 seconds. The local guardian receipt is COMPLETED/1/3.
- The separate signer exited 70 after terminal authority revocation, with its
  fixed input/phase-rejection diagnostic; this is recorded, not relabelled exit
  0. The main composition stopped gracefully. Future service packaging must
  distinguish terminal signer revocation from unexpected active-phase failure.
- Both disposable containers removed. No VPS/SSH, DB0, FINAM network/order
  execution or accepted O2 root/history change. Operational rows remain NOT_RUN.

Result SHA-256:
`1dbba38a817bcfbe7b794f185056c4c3f2ba5bd422eb30b602960373be4cd3c8`.
Seal SHA-256 before/after child restart:
`4012c8097cbe0529245d888de2781103a2328b2cb044f2ea5605895fbcd1711c`.

`components-run17`: 23 exact selectors, production binary build, fmt, strict
clippy and diff check PASS; 27 recorded checks total, source-stability and
13-row NOT_RUN checks PASS. Result SHA-256:
`ecbb7627048778a604f7df07e81f5eef0103ea600e70c7c398dd625da8044cc1`.

Supplemental local Cargo runs also passed: strategy-runtime-core 1302/1302;
runtime-durable-service 433 passed, 17 ignored helpers/optional harnesses.
Those full-suite summaries were observed in the execution console, not
retained as full immutable logs; the exact component gate above carries the
retained logs. No full CI, source acceptance or operational O3 PASS is claimed.
