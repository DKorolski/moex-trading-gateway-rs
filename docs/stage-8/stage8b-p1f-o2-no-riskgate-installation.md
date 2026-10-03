# No-riskgate O2 installation package (non-activating)

2026-10-01. Status: INSTALLATION_PACKAGE_REVIEW_CANDIDATE.
No installation, authority issuance, FINAM/Redis access or O2 execution in this work.

## Accepted inputs and exact predecessor

Binary artifact `64f1fd5d00993c01a3109a2f89c990466ac2f781` is independently
BINARY ARTIFACT ACCEPTED in `FINAM_64f1fd5_NO_RISKGATE_O2_ARTIFACT_REVIEW_2026-10-01.md`,
SHA-256 `3366ed7127ca0404d6f96614868c149d132552c999a976859fe2df7d640d8477`.
Artifact SHA `66de9f7b362c90ca457c3f0dfb921c7eaf31091ce4d3aa5bbf95268a3544cee3`;
compiled merge `ca1e5da7ea41eec219bce1cfe2bdf4b8d63d9029` is unchanged. No rebuilding.

The prior installed inventory SHA is
`295c49dffc688edbe1e5d1b79eaf0e9773b512db5150cf2180a51aab16f0247e`, reconstructed
from the exact `3923c5c` installation package. Retained bounded-attempt evidence
SHA `527d20dbc23079d9151f6868c81cab135b2db5c53fac2dce24fce64a83cac2d7` records
FAILED, generation 1 / sequence 4; event
`5d3dd63b7b26a538431268c4ef5f1b0a565fde4e31135ae1aa735d79d1b3f662`.
It is a retained observation, not a fresh target preflight or a claim of O2 success.

The genesis-bound `installation_id=stage8b-p1f-o2-baseline07-7196aaa-v1` is
retained. A new manifest SHA binds the new payload, predecessor, updater hash
and revision. The old sequence-2-only updater is unchanged and is not invoked.

## Eight exact replacements

1. `stage8b-p1f-o2-materializer` (accepted ELF).
2. `stage8b-p1f-o2-operator` (accepted ELF).
3. `stage8b-p1-paper-supervisor` (accepted ELF).
4. Public `o2/materialization-policy.json`: V2 no-riskgate profile, short interval.
5. Public `o2/source-template.json`: wire V3, explicit five-session calendar.
6. Public `o2/supervisor.template.json`: accepted V2 profile/fingerprint.
7. Compatibility `installation-v1.json`.
8. Full `installation-o2-v1.json` (last).

No unit files, authority keys, signed manifests, active selector, receipts,
genesis, durable state or staging output are replaced. File modes/owners/groups
and directory custody remain unchanged. The three public templates stay
root-owned, service-readable 0440. They are templates, not broker truth or
a issued source bundle; credential sentinel and empty materialization fields
remain explicit. No account/token/private signing key is read or generated.

## Reviewed-calendar candidate for 2 October

This package plans the next weekday session, **2026-10-02**, not an authorized
execution appointment. Four prior sessions: 2026-09-28, 29, 30 and 2026-10-01.
Their canonical M10 closes are 07:10–23:50 MSK, continuous, with full current
session windows declared separately. History is four sessions, not 15 days.

Policy fetch interval: `2026-09-28T04:00:00Z` to `2026-10-02T20:50:00Z`.
Its end is a fetch upper bound, not permission to consume future or incomplete
bars. Existing source selects the latest complete candidate at/before trusted
now; candidate age <=900s, broker truth <=300s. Current prefix stops strictly
before the candidate. Prospective execution must fall within
`2026-10-02T04:10:00Z`–`2026-10-02T20:50:00Z` and recheck actual bar freshness,
schedule/account truth and all accepted gates. Failure is not permission to
expand history, invent bars, change clearing windows or infer a data gap as a break.

Calendar basis checked on 2026-10-01:
[MOEX July session extension](https://www.moex.com/n101220),
[removal of intraday clearing breaks](https://www.moex.com/n98363),
[September main-session start](https://www.moex.com/n103379).
The declared continuous weekday window follows these announcements and the
accepted source-plan V3; no weekend session is introduced. These calendar
references do not prove FINAM's actual data coverage or an absence of an
exceptional instrument halt. Those remain execution-time evidence requirements.

If review/installation misses this session, **do not run stale inputs**.
Prepare a newly bound calendar/policy/template and installation manifest; no
silent rolling window, automatic issuance or runtime default is added here.
Source plan SHA is `d722d70a897578ce93217f34c82dff2a7ed6c6c402a12914b7b862c2d95b693c`;
profile SHA `6d7dff3543993b7727a161f85af3037f74762ecd2ce8332e0594b9c92d987c38`;
runtime fingerprint `665f18112142f6c9aee1b613e341870f0d559d783fe41df725e26415507e6290`.
The operational namespace identity is unchanged: changing this runtime profile
does not change the accepted operational-identity constructor's namespace fields.
The release-rlib probe checks that identity against the new supervisor inputs.

## Transaction and non-activation

The fixed successor retains the existing fsync/reread/journal/lock protocol and
imports the existing custody helpers. Its fixed constants and payload scope are
separate from the accepted historical updater; no generic recovery API is opened.
Transaction directory: `/usr/local/share/moex/stage8b-p1e/o2-no-riskgate-ca1e5da`.
Old transactions are untouched.

Before writes and between slots, preflight reads actual host/P0/stopped-process
state and exact authority/config/durable/staging inventories under the existing
execution and guardian locks. Expected FAILED/1/4 and all retained receipts must
match. P0 observation drift, pending claim/new selector, extra config, credentials,
unknown bytes, unsafe modes/links or lock contention fail closed. Only the three
declared public config payload hashes/sizes may transition old→new; their custody
and all other config entries remain exact. Manifest/state-specific file checks
still require exact old/new bytes, not arbitrary config values.

All eight backups and PREPARED are durable before replacement. Manifests are
last. `resume` handles only exact mixed old/new slots. `rollback` restores old
bytes and keeps history; authority advancement forbids both. Incomplete backup
preparation or unexpected transaction entries remain inspection failures, not
automatic deletion/repair. New config/manifest means no new phase is usable yet.

CLI supports preflight/status/apply/resume/rollback. It checks the complete
prepared package before opening target paths. Mutation requires an exact
confirmation phrase **and separate human authorization after package review**.
No daemon-reload/reset-failed/start/enable, claim/sign/genesis, Redis or FINAM
operation is implemented. Fresh preflight can legitimately refuse if the stand
changed since retained evidence; never relabel that drift as a passing snapshot.

## Local evidence and limits

The package gate retains exact commands, exit codes, deadlines and logs:
14 native ARM Linux/root filesystem tests, 16 exception-after-fsync/reopen
frontiers, 10 prepared-input negatives, 45 authority negatives; accepted-release
rlib input/identity/calendar validation and actual amd64 ELF input admission.
Only the private fixture filesystem is mutated. Host/systemd observations are
mocked in filesystem tests. These are not SIGKILL/power-loss tests or native
amd64 VPS execution. Docker tests are network-none, without privileged mode,
host root mounts, credential mounts or Docker socket.

The ARM filesystem image is the already-retained local image
`sha256:9bec3cfe280a732eea281cb1add79f75523d1fe2ac86c12b129e4bc2509ec163` (not claimed
registry-pullable); the ELF/probe image is the accepted pinned Rust image.
No new systemd mount proof is needed because unit bytes/mounts are unchanged.
No production Rust/Cargo/workflow change or new full Rust-suite result is claimed.
Use actual gate result, not this declared test inventory, as the PASS evidence.

## Handoff and next permission boundary

Full source ZIP includes `installation-no-riskgate/` (prepared eight slots,
spec, source plan, accepted binary artifact/review, prior installation and retained
terminal evidence), plus `installation-review/` (raw commit/tree inventory and
gate evidence). Verification is read-only and works without Git:

```sh
python3 scripts/stage8b_p1f_o2_no_riskgate_install_review.py check /absolute/archive.zip
python3 scripts/stage8b_p1f_o2_no_riskgate_install_package.py check installation-no-riskgate
python3 scripts/current_tree_authority_check.py
```

Next: independent installation-package acceptance → separately authorized
non-activating installation/fresh before-after evidence → separately authorized
bounded O2. FINAM write, broker dispatch, unattended runtime-live and real orders
remain closed. No extra source stage or recovery framework is proposed.
