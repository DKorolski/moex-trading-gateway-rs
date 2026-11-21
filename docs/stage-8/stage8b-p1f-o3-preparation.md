# O3 preparation — existing-root synthetic paper / restart

2026-10-08. **PREPARATION / REVIEW CANDIDATE. NO OPERATIONAL AUTHORITY.**

This is a narrow implementation-readiness package under the accepted P1F plan,
not another strategy design, a new recovery framework, or an executable VPS
installation. O2 is closed; O3 operational acceptance remains pending.

## Accepted boundary

The [O2 review](reviews/REVIEW_2d67937_O2_BOUNDED_20261008_2004_RU.txt) accepts
installation `2d67937f5f6d70297e7b461649f45a4a8e75641a`, compiled source
`09dfecbe931267c5276b8b805d62d5a5a141ad95`, and operational evidence ZIP
`moex-trading-project-2d67937-o2-bounded-20261008-2004-evidence.zip`, SHA-256
`15b0210cc7e5ecab29e1fc5460559fe46ce21a1e6d13def0d1b591a16fc57a0e`.
Review SHA-256: `8ee83a7b49ed6999ca3700d60b8ce0f3e8a8b6eec60d19ad3dbf86be973cd8b9`.

Last accepted observation: October 8, 20:14:13 MSK. COMPLETED/1/26, event hash
`fb4fa038d829f0d21e5f2d8e39b1bd26e05ba4f4186b95dfa6205b41d47933d8`.
Operational identity:
`fbd9ceb7ee964944e79db559c4e4d5cc7af54578b653ce218e183d01fa1444c3`.
Profile `imoexf-baseline07-bo-only-no-riskgate-paper-v2`; lifecycle key stays
in its existing custody. Root at `/var/lib/moex-finam-p1-paper/state`:
adopted marker generation 5, receipt generation 1, seal generation 1,
authoritative Replay callbacks 1, requests 0, Stage6 frames 0 (10-byte header).
P1 stopped; DB15 empty; P0 runtime/WS running. These are dated evidence facts;
fresh read-only preflight must precede any future authorized mutation.

## Already implemented, versus still missing

| Component | Existing implementation | Preparation conclusion |
| --- | --- | --- |
| Ordinary authenticated readmission | `stage8b_p1e_process.rs`, `admit_stage8b_p1e_ordinary_run_v1` | Reuse the same root/key; no second bootstrap or Replay. |
| Phase ownership / bounded child | `stage8b_p1f_local_supervision.rs`; `stage8b-p1f-local-supervisor run SHA` | Source exists, but this executable is not in the accepted O2 installation inventory. It is not an installed O3 runner. |
| O3 phase claim | Guardian `claim_phase` | O2 operator `guardian-claim-fixed` subsequently calls `publish_o2_active_manifest_selector`; it must NOT be repurposed for O3. Add only a fixed O3 claim composition, without replacing authority/genesis. |
| Signed schedule / M10 publishers | `stage8b_p1f_fixed_producers.rs` and `observed.rs` | Existing production libraries, not an installed synthetic-producer daemon. Use generation-2 schedule signing; do not use a test signer on VPS. |
| Fresh observed-M10 admission | `observed_published_window` → `execute_stage8b_p1e_observed_run_v1` | Typed production seam exists. No executable currently calls it. Ordinary CLI `run` supplies `None`; it does not discover new authority from Redis or filesystem hashes. |
| Settlement / recovery | P1-d2/d3/d4 and P1-e owner route harnesses | Reuse; tests on disposable roots do not prove readmission on the adopted O2 root. |

These are concrete composition gaps, not regressions reopening O2 acceptance.
The local gate records Cargo binary inventory, installed payload paths and
exact source checks. In particular, the observed entrypoint has one definition
and zero executable call sites at the accepted source baseline.

## Smallest next implementation slice

1. Compose a fixed O3 entry with the accepted guardian APIs, bound to the exact
   existing terminal predecessor. No generic command runner, new authority
   service, key rotation, root migration or O2 materialization.
2. Wire accepted synthetic schedule and **observed V2 M10** producers to the
   ordinary observed-run seam. Preserve process signal/credential/telemetry/
   S06 handling. The current child-only supervisor launches legacy CLI `run`;
   a typed window cannot cross that exec boundary implicitly. The implementation
   must demonstrate an explicit checked producer-to-owner handoff (and exact
   restart re-admission), not bypass the window check or deserialize a capability.
3. Qualify one minimal fixed runtime bundle and service/Redis/signing composition
   only after the local linked path works. Preserve accepted root, key, O2
   history and P0; update installed inventory explicitly if bytes change.

Library source acceptance is reused, not relabelled as O3 artifact acceptance.
The existing sparse path has source generation **2**, not the historical dense
V1 generation 1. Schedule key generation 2 is a separate identity. Retained
producer receipts/high-water must never be inferred solely from stream fields.
On restart: separately pinned synthetic source plus exact retained receipts,
same Prepared/Published bytes and sequences; loss/conflict fails before fresh
callback/publication. Old O2 history remains the seed, not live producer authority.

The signed schedule signer interface already exists; the actual constrained
AF_UNIX signing process/credential availability is a packaging prerequisite,
NOT proved by the local fixture signer. No ceremony/key access is done here.
The O3 package must pin the exact synthetic fixture and source range, use the
accepted no-riskgate model unchanged, and declare synthetic provenance honestly.
Do not move host time or edit retained source/config fingerprints to make a
historical fixture current. If the minimal adapter needs review, review that
concrete code/evidence delta, not accepted historical stages again.

## Evidence contract for the bounded O3 run

The [matrix](stage8b-p1f-o3-acceptance-matrix.csv) separates inherited local
witnesses from future operational proof. None of its O3 execution rows are
marked PASS by this preparation.

- Fresh readmission of the accepted root retains the initial Replay count 1.
  Later legitimate callbacks advance from that baseline; no new first-boot ID.
- Fixture-produced canonical M10 traverses the production callback and paper
  provider, Stage6 outcome, Stage7 RequestFinalized, ACK/S_ack, truth/S_truth,
  persist+reread+cross-validation, then source XACK last.
- Bind every capture to phase, operational identity, runtime/config fingerprint,
  source/M10 IDs, producer sequence/high-water, request/idempotency identity,
  callback/provider-effect counters, seal generations and source PEL state.
- Six accepted scenarios remain: clean stop, SIGKILL at pending M10 before
  effect, stale PEL reclaim, exact duplicate, conflicting duplicate, uncertain
  provider outcome. XACK-response-loss / AlreadyAcknowledged is an additional
  required witness, not presumed from a normal exit.
- Use existing local frontier harnesses as inherited proof. On VPS inject only
  faults supported by a reviewed ordinary production mechanism. A test-only
  barrier or ignored Cargo test is NOT a production O3 launcher. Any frontier
  not operationally demonstrated must be labelled inherited-only, not PASS.
- Stop P1 only; preserve durable state and all authority/terminal history.
  No DB flush/delete, consumer rewind, source XACK before truth, or P0 changes.

## Operational issue gate (NOT issued here)

Before signing/activation: accepted executable package; exact adopted-root /
history / installation / P0 read-only preflight; exact fixed Redis DB15 groups
and keys provisioned by the existing initializer only; signer identity checked;
fixture/input scope and fault order fixed; explicit operator permission.
DB0 is never a P1 target. DB15 emptiness is the O2 observation, not proof that
O3 groups are already provisioned. Accepted source attach is verify-only.

Use a new O3 manifest under existing authority; proposed sequence 27 is valid
ONLY if a fresh head is still exact COMPLETED/1/26. Generation is not reset.
No signing/private-key read, VPS write, timer, source feed, run date or active
phase is created by this preparation. Existing consumed O2 phase is never reused.
Maximum O3 duration is 30 minutes from claim, original deadline never extended;
existing five-start/600-second child restart budget and 30-second authority
stop grace remain. Explicit clean stop before deadline can complete the phase;
deadline yields Expired, fatal/conflict scenarios Failed as defined by guardian.
A terminal phase cannot be reused for the next destructive scenario. If multiple
phases are necessary, enumerate them and get approval; no automatic fresh claim.

## Local qualification and delivery

`scripts/stage8b_p1f_o3_preparation.py` reads the accepted installation/O2 ZIPs,
checks their hashes, checks the source gaps, and runs selected existing exact
Rust witnesses with ephemeral local Redis. No SSH/systemd/FINAM/operational
Redis action is implemented in this tool. It records commands, exit codes and
complete combined output. This is targeted preparation, not a full Rust-suite,
new GitHub CI run, Linux artifact build or proof on the VPS root.

The handoff includes exact committed source, this matrix, accepted O2 review,
the small accepted O2 evidence ZIP, local evidence and source-tree hash index.
No old installation ELF/large archive, secrets, .env or private keys.

After O3: separately bounded O4 FINAM paper windows, including the retained
[operational market-data checklist](stage8b-operational-market-data-parity-checklist.md)
(WS subscriptions/reconnect, freshness, REST overlap, session/EOD and frozen
ALOR comparison), then P1F aggregate acceptance. FINAM order writes, broker
dispatch and real orders/live micro remain forbidden throughout this package.
