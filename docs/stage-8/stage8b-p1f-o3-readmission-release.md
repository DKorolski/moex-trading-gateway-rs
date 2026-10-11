# O3 readmission release preparation

2026-10-11. Native production build PASS; unified release package IN PROGRESS,
not a completed or accepted release/install package.

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
