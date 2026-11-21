# O3 readmission release / isolated native qualification

2026-10-12. Review of `45a3f1b` is **RELEASE HOLD** for installer retry
durability (P1-O3-REL-R01), plus the atime reader issue (P2-O3-REL-R02).
Production source and isolated native start/stop qualification retain acceptance.
The local installer-only correction below is ready for focused re-review;
it does not independently grant release/installation or operational acceptance.

## Installer retry correction — review candidate

The [independent review](reviews/REVIEW_45a3f1b_O3_RELEASE_RESULT_RU.txt)
is retained. Before a terminal journal, apply/resume and rollback now
verify desired files, fsync all distinct replacement parents and TX.parent,
then reread files and check invariants. This barrier runs even for exact desired
bytes. A failed barrier cannot write a new APPLIED/ROLLED_BACK journal.
Incomplete mkdir preparation also resyncs TX.parent before PREPARED; a visible
interrupted journal is resynced before replacements. Existing locks, custody,
backup validation and protected-state checks remain in place.

`package_file` compares dev/ino, mode/nlink, uid/gid, size, mtime_ns and ctime_ns
across fd and path. Only atime is excluded; ordinary reads may change it.
Regular-file/single-link/size checks remain unchanged.

Four new tests fail against the original installer (three failures plus the
atime error), while its existing 19 tests pass. All 23 tests pass after correction
in a local network-none Linux/arm64 container, using real files, locks, rename
and fsync with synthetic host observations/DBSIZE. Apply and rollback tests inject
EIO directly before the last ELF parent fsync, then fail the final retry barrier,
then prove an exact-bytes/no-payload-rewrite retry syncs parents before terminal.
Incomplete mkdir retry and a real read-induced atime update are covered too.
These are filesystem fault witnesses, not power-crash or new native SIGKILL proof.
Both before/after logs and commands are retained with source/spec/log hashes.

No VPS contact, installation, schedule-key use, production/helper rebuild or
native rerun was performed for this correction. Five production ELFs, both
units, both qualified helpers, baseline and executed driver/setup/collector are
unchanged. The original native result remains byte-for-byte SHA-256
`59ac5446806d22a56648ad3b5057e1588f937c05e01c927b37f4833af1ac5533`.
Its original `installation_spec_sha256` remains
`f19ff3a430c4e49b697a50005333cb16f7ecaeaed16557b370af340ce2dc175b`.
The corrected installer necessarily derives new manifest/spec hashes. The ZIP
contains `qualified-installation/` separately from `installation/`; the verifier
allows only installer hash and its dependent manifest metadata to differ.
It rejects changes to ELF slots, baseline, original native result, or any other
spec/manifest semantics. Eight continuity tests supplement the retained 20
native/helper mutations (28 total); runner15, builder9 and templates10 pass.

The 07:37:31Z protected snapshot used for local candidate derivation is retained
dated evidence, not a fresh installation preflight. Authority changes only the
current-status inventory entry and aggregate hash; policy/workflows stay unchanged.
Next: focused release/install re-review, fresh preflight and separately permitted
stopped installation, installation-evidence acceptance, then separately permitted
bounded O3. O2 remains accepted; operational O3 remains NOT_RUN. O4 includes
WS/reconnect/freshness/EOD and multi-session ALOR parity before live micro.

The remaining sections retain previous accepted evidence and historical attempts.

## Corrected fixture and one permitted repeat — PASS

Production remains the five exact ELFs built from accepted
`5df1c88299bbb7f1a70b709d3c01e2cf0a09d08c`; no production Rust/Cargo, unit,
installation or runtime permission was changed. Only the disposable example
`o3_local_signer_witness` was corrected at
`d26c02a378b0c5b33af0bcfad8e0224d5f0e2ad9`: its truth marker is now
`SYNTHETIC-O3`. A regression calls the actual production paper-source constructor:
the corrected fixture passes, while the old marker fails even with a recomputed
fixture hash. This test passed locally and in the native release build.

The separate helper-only build record is `helper-build-r1/helper-build.json`,
SHA-256 `8496dfff89ca7df5489603aed28238fce0756d3f8373e2ad8495d546f4ee206a`.
Its sole ELF is `input-helper`, SHA-256
`72175add8a0404d263bee903510b7473c6af88883fef26eaaff7a30ebf5821e4`.
It is explicitly `FIXTURE_HELPER_BUILT_NOT_RELEASE`, with zero production
artifacts. Original production and support records are retained unchanged;
the bootstrap helper is reused, the original input helper is historical only.
Current-tree authority inventories include the example correction; authority
policy and CI workflows are unchanged.

After native build cleanup, fresh preflight at 07:32:23Z matched the protected
host baseline. The owner-permitted repeat entered the ordinary Monday window
at 10:36 MSK, without a clock override, schedule or automatic retry.
`native-run-r1/result.json` has SHA-256
`59ac5446806d22a56648ad3b5057e1588f937c05e01c927b37f4833af1ac5533`:
`NATIVE_SYSTEMD_START_STOP_PASS_NOT_OPERATIONAL_O3`, 72 command records.
The nine public payloads bind the five unchanged production ELFs, two unchanged
units, unchanged bootstrap and separately corrected input helper. The actual
driver is SHA-256 `c163782c3404a0b82a685fcbfbe76e38291ce0d7b72ea29ee968acf0295fab1d`.

Actual production bootstrap, signer admission/signature verification and fresh
isolated Redis initialization passed. Signer and supervisor/UID987 child remained
active through the stability observation without restarts. Bounded supervisor
stop took 0.461s: `GRACEFUL_STOP`, one child start, child exit0, no forced kill.
The signer then exited70 at the terminal boundary. All recorded PID/start-time
identities disappeared. This is start/stop qualification, not completion of the
operational O3 decision/successor scenario.

The sole existing schedule credential was verified against its public key,
transferred over SSH stdin only into container tmpfs, and explicitly unlinked
after the services stopped; the systemd credential directory was absent.
No private key bytes, digest or custody path are retained in this package.
Both disposable containers and public staging were removed. Host Docker/binfmt
inventory was unchanged. Protected postflight at 07:37:31Z matches P0, installed
bytes/config, prior transactions and O2 COMPLETED/1/26; host DB15 remains empty.
No operational services, installed package, DB0 or terminal history were changed.

Offline evidence checks pass: 20 native/helper positive-and-mutation tests,
15 runner tests, nine builder tests, ten source/template tests, authority and
45 authority negatives. Retained 19 installer tests are Linux filesystem tests,
not a new native SIGKILL matrix. `cargo fmt --all -- --check` passes. Full CI and
full Rust suites are not rerun or claimed: accepted production source/ELFs are
unchanged and the only Rust correction is the regression-tested fixture example.

Handoff command (local, no network or operational effects):

```sh
python3 -B scripts/stage8b_p1f_o3_readmission_handoff.py package OUTPUT_DIR \
  --work RELEASE_EVIDENCE_DIR --native QUALIFICATION_DIR/native-run-r1 \
  --helper-build QUALIFICATION_DIR/helper-build-r1 --correction INSTALLER_CORRECTION_DIR
python3 -B scripts/stage8b_p1f_o3_readmission_handoff.py verify ARCHIVE.zip
```

The archive carries full tracked source, exact package/accepted/helper Git trees,
build records and compiler graphs, seven-slot installation candidate, historical
failed attempt, new native observations and focused offline gate logs. SHA-256,
safety and review-message sidecars accompany it. The original helper build is
not relabelled as the corrected build. Missing/failed native evidence blocks
packaging. Next: independent release/install review, separately permitted stopped
installation, installation-evidence acceptance, then separately permitted bounded
operational O3. No new recovery framework or main/CI merge is required for this
isolated qualification.

## Earlier Monday native attempt — FAILED / NOT QUALIFIED (historical)

Retained operation directory: `reports/operations/o3-readmission-qualification-20261012`.
`before-preparation/record.json` and `stdout.json` record the fresh read-only
preflight at 06:06:00Z. It matches accepted P0/O2/config/installed bytes/prior
transactions, COMPLETED/1/26, DB15 empty and synchronized NTP.

Before attempting the native run, a concrete harness custody bug was fixed:
`docker cp` retains the staged public `/payload` directory's 0700 mode, while
production bootstrap executes under UID987. The container-only directory and
the two public support executables now have 0755; executable/not-writable checks
run as UID987 before fixture generation. No host paths, credential permissions,
unit templates or production binaries were altered. Twelve offline runner
tests pass, including this directory traversal regression. `prepared-r1`
contains the exact corrected driver/public inputs; the Sunday prepared-r0 is
not relabelled.

`native-run-r0/result.json` and its individual command logs retain the single
authorized run. Actual fresh synthetic inputs were generated at
2026-10-12T07:06:55.103320Z. The new production bootstrap succeeded. ELF hashes
were checked twice. The selected existing schedule credential was verified
locally and transferred only through SSH stdin into isolated container tmpfs.
Signer systemd syntax/loading/start exec succeeded, but its readiness check
found a failed service: application exit70, `O3 fixed input, custody or phase
binding is invalid`. There was no initial-schedule request, supervisor start,
positive composition child or start/stop PASS. This exit70 is a startup refusal,
not the expected terminal refusal after a successful run.

Cleanup removed both uniquely named containers and the public staging directory;
the credential tmpfs disappeared with the signer container. The normal successful
path's explicit credential-unlink milestone was not reached and is not claimed.
Host Docker/binfmt inventory and protected state before/after match exactly.
`cleanup_notes` is empty; installation and operational O3 remain false.

Static diagnosis found an independently sufficient incompatible input:
`crates/finam-gateway/examples/o3_local_signer_witness.rs` emits broker asset
marker `LOCAL-SYNTHETIC`, but the accepted
`crates/strategy-runtime-core/src/stage8b_o3_readmission.rs` constructor requires
`SYNTHETIC-O3`. The linked helper's synthetic truth becomes the composition
plan verbatim. The signer invokes this constructor through `load_fixed` before
reading the service credential or binding its socket. The generic error does
not independently identify the first failed predicate; no claim that this is
the only remaining issue is made. Do not weaken the production restriction.

The narrow next step agreed after this failure was to correct/regression-test and separately rebuild only
the disposable input helper, retain its distinct source/ELF provenance, then
one newly permitted isolated qualification using the same five production
ELFs and unchanged units. No automatic retry, source acceptance relabelling,
host installation, clock override or operational activation. The original
helper/build/run evidence remains immutable. The new packaging script and its
positive-evidence mutations were drafts until a successful new witness existed;
the assembler rejects the current failed result before running package gates
or writing a ZIP. No final review ZIP existed at that failed-attempt checkpoint.

## Preparation checkpoint (2026-10-11; historical)

Accepted source: `5df1c88299bbb7f1a70b709d3c01e2cf0a09d08c`.
Source tree: `d0d57aab9079b4afe5941bf414e29ff75f19c860`.
The independent source review closes P1-O3-R01 and P2; no new source audit,
runtime architecture or expanded fault matrix is being introduced.

## One successor package

Build the same five production slots using the existing offline, pinned
Linux/amd64 Rust image and exact Git export. Preserve production Cargo artifact
messages, source tree and ELF hashes; fixture setup binaries are separate.
Do not inherit the old `fb1ceb3` native start/stop success as evidence for these
new ELF bytes. The old release documentation and evidence remain historical.

Refresh only current-tree authority inventories, not policy/workflows. The
installation successor must match the already installed O3 package plus the
accepted `2be020c` config correction, not the older absent-O3-slot baseline.
Retain the corrected supervisor config, O2 root, receipt/history and P0.
Do not reuse the old install transaction directory or silently reinitialize
Redis. Preparation grants no permission to install or activate.

The fresh inventory pins the installed outer manifest to
`c2585bb68d67aa8515db15762d61fdd5cf5cd2e36bf75cdec3023339ac937c15`
and the installed O3 manifest to
`3075cc2cbf96a70d75684630ca9a61a01e274f96e61bf84c13fe143448ce0ca0`.
Both match the retained native installation payloads byte-for-byte. The
successor's intended delta is five ELF slots plus these two manifests;
the existing signer/supervisor templates are retained, not rewritten or
loaded. The corrected supervisor config remains
`3a031f24ac91990bb4b00308fad93d48b36af09d135ab5d4ecc62e4702e5c3e8`.
The stopped successor installer is now prepared locally and filesystem-tested;
it remains a release-review candidate, not authorized installation.

`scripts/stage8b_p1f_o3_readmission_install.py` pins the exact native build
record and accepted protected baseline, derives seven replacements and reuses
the existing two locks, secure reads, atomic replacement and fsync primitives.
The new transaction has its own directory; prior installation/config journals
are checked and never rewritten. A partial backup stage resumes only while
all installed slots are still exact-old. Once PREPARED, apply/resume and rollback
accept only exact old/new bytes; foreign files, corrupted backups, nonempty
DB15 or state/config/host drift stop the operation. No flush, reload or activation.

Nineteen focused Linux filesystem tests pass (including each of seven backup,
replacement and rollback boundaries). These use real accepted predecessor and
candidate payloads, file ownership/modes, locks, rename and fsync inside a
network-none local Linux/arm64 container; host/systemd observations and DBSIZE
are synthetic. Exceptions are injected after durable writes: this is not a
claim of new SIGKILL or native ELF execution coverage. A malformed journal
case initially surfaced a KeyError; the installer now rejects it explicitly,
and the regenerated candidate passed the full test run. Retained logs describe
both the stopped initial run and successful rerun.

The exact plan must include `catchup_coverage` under the accepted source
contract (opening model labels despite legacy `*_close_time_utc` names).
Regular session coverage must be explicit; weekend suppression follows the
frozen `weekends_off=true` model. Missing regular data is not a holiday.
The current input helper's `prepare-linked` is weekday/real-model-window only;
do not override its clock or widen the frozen calendar for a weekend test.
An expired old plan/phase must not become new operational authority.

## Current evidence and boundaries

Fresh read-only VPS preflight at 2026-10-11T05:13:33Z passed against the
accepted config-installation snapshot: protected state/installed bytes/P0
match, O2 COMPLETED/1/26, stopped P1, DB15 empty, NTP synchronized.
No credentials were read and no remote writes occurred in that observation.
This is dated evidence, not a substitute for preflight immediately before
any later authorized installation or execution.

Local production build could not start: QEMU segfaulted during `rustc
--version --verbose`, before Cargo compilation. The failed command/log is
retained; it is not a source regression or a build PASS. A second cached Rust
image also failed the toolchain probe. Only the two task-owned containers were
stopped; Docker/ALOR/binfmt configuration was not modified.

The owner authorized read-only VPS inspection and one isolated native
qualification, without operational O3 or installation. On 2026-10-11 the owner
also explicitly authorized native compilation in the cached pinned image,
limited to 1 CPU and 2 GiB RAM/no extra swap, network-none, public inputs only.
Only task-owned temporary build paths and a labelled build container are used;
the accepted host installation is not a source/cache mount. Compilation is not
the later systemd qualification and neither is operational O3.

The first native build attempt was interrupted by an SSH connection timeout
while reading `docker inspect` (`state-24`, exit 255), not by an observed Rust
compiler error. Its result is FAILED, not a successful release. The last
retained state was running/not OOM-killed; the compiler progress observation
had reached runtime-durable-service. Full compiler output was not recovered
before removal, so completion cannot be inferred. Both task-owned container
and temporary build directory were removed; a fresh read-only check at
2026-10-11T06:03:12Z matched the accepted protected state and installed bytes.

The builder now allows at most three attempts for read-only `docker inspect`
on SSH exit 255 / observer timeout. Docker errors and invalid JSON are not
retried; create/start/write/sign operations are never automatically replayed.
An interrupted build attempts to retain its public compiler output before
cleanup. Seven offline input/polling/profile-separation tests pass. The fresh native rebuild passed
using the same accepted source, image, public-input boundary and resource limits.

The retained `native-build-r1/build.json` has SHA-256
`e82171c248fda803eb973a141642a8510a77490c40dbd75e95bf471f8b54540e`.
It records five x86-64 production ELF hashes, 189 compiler artifacts without
fixture/test features, exact source export reverified before/after, and cleanup
of the unique build container/directory. Pinned image toolchain is rustc 1.98.0,
host x86_64-unknown-linux-gnu; offline release compilation exited 0 in 698.221s.
Build SUCCESS is not dynamic systemd or operational acceptance.

Read-only postflight at 2026-10-11T06:19:27Z matches the accepted installed
bytes, protected trees/history/P0, stopped P1 and both earlier installation
transactions. O2 remains COMPLETED/1/26; DB15 is empty. A separate cleanup
observation finds no remaining Docker containers or task staging directories.
Local `checkpoint.json` and its verifier retain source tree/commit, ELF/log
hash, production graph, host-before/after and cleanup consistency checks.
These files reside under `reports/operations/o3-readmission-release-20261011`;
public Cargo/source transfer archives are build inputs, not review ZIP contents.

Local package checks pass: native-builder boundary/polling 7, source/template
compatibility 10, historical release-verifier compatibility 12, authority
checker and authority negatives 45/45. The 12 historical checks use accepted
0e89208 only to show that its verifier remains compatible; they do not qualify
the newly compiled binaries. No new final release ZIP is claimed yet.

The two qualification-only helpers are also built from exact accepted source.
`native-support/support-build.json` SHA-256 is
`cc6c2fead646157b5cf06507fb20ff1eaca84966b3140b02470dd769285e88b3`.
Production bootstrap compilation exited 0 in 464.025s with 145 non-fixture
production artifacts. The input-helper example was compiled separately in
509.593s; its fixture-feature library graph is retained and explicitly not
release evidence. It is never installed into any of the five production slots.
The support build does not produce a `build.json` consumable by the installer.
Both helpers are x86-64 ELF; their output digests and source revalidation are
recorded. The temporary build container and staging directory were removed.

Fresh postflight at 2026-10-11T06:48:36Z matches the same accepted protected
state, installed files and earlier transactions; P0 is unchanged, P1 stopped,
O2 COMPLETED/1/26 and DB15 empty. The local candidate is
`installation-prepared-r1`; its new outer manifest SHA-256 is
`66301120137fbd4751ef10b5804f79ca0aabc4856c7f6f5271f130ddb8dd1605`.
This is a prepared replacement hash, not the currently installed manifest.

## Manual qualification runner (prepared, not executed)

`scripts/stage8b_p1f_o3_readmission_qualify.py` follows the earlier accepted
native start/stop recipe with the exact new five ELFs, unchanged two templates
and two separately pinned support helpers. The historical verifier remains
unchanged; a new run cannot inherit its PASS. The shared public fixture setup
is retained in `stage8b_p1f_o3_native_fixture.py`.

`prepare` is offline/public-only and refuses a private-key argument. The local
`qualification-prepared-r0/preparation.json` records all nine payload hashes,
runner/setup hashes and the exact read-only collector. It says
`PREPARED_ONLY_NOT_QUALIFIED`; no native result or final review ZIP is implied.
Eleven focused offline tests pass, including rejection before any SSH, input
loading, key read or output-directory creation on a Sunday; exact artifact/
template/collector binding; protected baseline validation; and redaction of
unexpected private-transfer output and local key-custody errors.

`run` requires an explicit isolated-test authorization flag and local selected
credential path. It rechecks the actual local/native UTC clocks and the exact
protected host baseline, uses no host mounts or broker network, and transfers
only the selected schedule seed through SSH stdin into container tmpfs. No
credential bytes, digest or local custody pathname enter the retained evidence.
Each effect is attempted once; cleanup addresses only uniquely named task
resources, including ambiguous create outcomes. The in-container six-minute
backstop is retained from the previous test recipe, not a host launch timer.
Postflight must match P0/O2/root/config/installed files and prior transactions.

The manual entry window is an ordinary weekday after 10:00 MSK, in seconds
360..479 of a ten-minute interval (for example, start around 10:06 MSK).
The narrower entry slot leaves preparation margin for ordinary bootstrap TTL.
Time is checked again before fixture input generation. A missed window fails
closed; the runner does not wait, retry the next session or override the clock.
Preparation on Sunday has not executed the runner's `run` branch or contacted
the VPS, and has not read or transferred the schedule key.

## Remaining acceptance boundary

Remaining package work: run the bounded native start/stop witness
in a real weekday model window; then seal one immutable
release/authority/install review ZIP. Existing old
support binaries, installer predecessor or native success cannot be relabelled
as current evidence.

The owner separately authorized the selected existing generation-2 schedule
credential for that isolated qualification only. Local correspondence to the
accepted public key was checked successfully; no signature or key transfer has
occurred. The custody path/private bytes are omitted from review artifacts.
This authorization is not a production ceremony rebind or operational issuance.
The existing `prepare-linked` input generator rejects Sunday (2026-10-11).
Positive systemd qualification therefore remains pending a real weekday model
window; neither synthetic clock changes nor bypassing this input guard are
part of this package. No automatic next-day launch is scheduled.
The next calendar candidate is Monday 2026-10-12, in the helper's ordinary
post-10:00 MSK model/fresh-bootstrap window, subject to a fresh preflight.
This is a manual qualification target, not a reserved phase or unattended run.
No new phase, timer, public schedule, credentials, DB0/P0 change or live order
is authorized by this document. No push/merge or full CI rerun is required at
this intermediate checkpoint.

## Resume checklist (preparation only)

The accepted source branch through `5df1c88` is now backed up on `origin`.
Local `main` is synchronized to `origin/main` at `ab0235a7`; this is not an O3
merge. Preparation changes and historical evidence must not be represented as
new release acceptance. Build caches removed on 2026-10-11 were limited to four
old CI `target/` directories; current O3 artifacts and retained evidence remain.

For the next manual native test:

1. Reverify `installation-prepared-r1`, `native-build-r1`, `native-support` and
   `qualification-prepared-r0` under the retained operation directory. Reuse
   the exact accepted source and ELF bytes; no new build is needed.
2. Use the existing runner in a real weekday entry slot (first calendar
   candidate: 2026-10-12 10:06 MSK). The selected credential is provided only to
   `run`, never copied into documentation, command logs or the package. The
   runner performs a fresh read-only host preflight before any remote effect.
   Do not run a missed slot repeatedly or schedule a background attempt.
3. Require the actual native start/stop result, recorded child identities gone,
   task-container and credential cleanup, and an exact protected-host
   postflight. An old PASS, a prepared input set, or an incomplete/failed run
   is not a substitute for these results.
4. Complete the readmission packaging/verification path using that result.
   The existing `stage8b_p1f_o3_release_handoff.py` is still pinned to historical
   `fb1ceb3` and its nine-slot installer. It must not be used to claim a new
   five-ELF/seven-slot `5df1c88` release by changing historical evidence labels.
   Preserve its historical verification path and include the exact new source,
   production/support build records, seven-slot install specification, fresh
   qualification, authority checks, SHA-256 and archive safety in one package.

The checkpoint verifiers do not contact the VPS. `verify_checkpoint.py` is the
historical one-shot producer of `checkpoint.json`, not a repeatable check:
running it again refuses to overwrite that file. For a repeatable full public
preparation check use the retained `verify_preparation_checkpoint.py`, which
compares the existing result and does not overwrite it.

Next after release acceptance: stopped installation with preservation
evidence, then separately permitted bounded O3. After O3: O4 actual FINAM
read-only WS bars/subscriptions, reconnect, freshness, REST overlap, EOD and
ALOR paper parity before any separate live-micro admission.
