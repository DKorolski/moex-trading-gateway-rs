# O3 actual-O2 config binding correction — review candidate

2026-10-09. Accepted predecessor: `0e89208c1e6235eab8b5ea0e115d7899cbafd223`.
Release and actual stopped installation are independently accepted. The owner
authorized one bounded O3; fresh preparation stopped BEFORE claim/signing or
Redis writes. This correction is prepared locally, **NOT INSTALLED / O3 NOT_RUN**.

## Finding and exact delta

Fresh read-only observations at 17:11 and 17:28 UTC matched the accepted installed
inventory, root, first-boot receipt/marker, history COMPLETED/1/26 and P0. DB15 was
empty. Exact installed supervisor config SHA:

`2d28d3c2d9fb3e4933a2f89e3b751dfe67d56b14d1e4f542bbc838e0513c56d6`.

Its Redis-manifest pin `3c9ddebe1395e5fdcf306be67988698fd2f1a20e1ae8c19e376e16027956ebbc`
does not match the unchanged production constructor for the current V4 identity
and no-riskgate profile:

`4b9162f0f87d55f4cca3b8efac9d33491f0d3f421eb090e8cdd78fcb0f3e86b3`.

The provisioner rejects that mismatch before opening a Redis client. Prior O2
bootstrap acceptance did not exercise O3 provisioning. The accepted local O3
witness calculated its manifest dynamically for a disposable root; that did not
prove compatibility with the inherited operational config. No VPS defect or key
loss is implicated.

Correct ONLY `redis_deployment_manifest_sha256`; new config SHA:

`3a031f24ac91990bb4b00308fad93d48b36af09d135ab5d4ecc62e4702e5c3e8`.

All other JSON fields and canonical encoding remain exact. In particular,
operational identity, runtime profile/fingerprint, bootstrap/source binding,
Redis URL/DB15, generations, namespace, health and shutdown policy are unchanged.
No Rust/Cargo, production ELF, unit template, installed release manifest or
workflow changes. No permissive fallback or disabled SHA comparison.

## Preserve history; describe a new config revision

The old materialized-set receipt remains a truthful record of O2's original
config bytes. It is NOT rewritten to pretend O2 materialized the corrected config.
All authority events/receipts, history-head, durable root, seal/journal, first-boot
receipt/marker, HMAC, source bundle and previous installation transactions remain
exact. No bootstrap, genesis/rebind, migration, clock reset or Redis reset.

The new narrow administrative record is outside authority history:

`/usr/local/share/moex/stage8b-p1e/o3-config-binding-after-completed-26-v1/`

It retains original config bytes plus PREPARED/APPLIED journal binding both config
hashes and the accepted installation observation. Existing secure custody,
atomic replace/fsync/reread and execution/guardian lock inodes are reused.
Only the live `/etc/moex-finam-p1-paper/supervisor.json` is replaced (root:paper
0440); the backup/journal are root:root 0600 in root-only 0700 directory.
No enable/reload/start, signing, private-key read or Redis mutation is available.
Fresh preflight also requires empty DB15 and absent operational O3 inputs/units.
An interrupted same correction can be explicitly resumed, not automatically
retried; foreign bytes, custody, history or installed-payload drift fail closed.
An interruption leaving an unknown atomic temporary file is a fail-closed manual
inspection case; there is no generic directory cleanup/repair mechanism.

This is a documented, owner-authorized preparation of a config baseline change,
NOT an implicit allowance to normalize future differences. Installation still
requires review and explicit permission. After accepted apply, fresh O3 inputs
must bind the new config SHA and exact unchanged root/history; the old preflight
snapshot is not silently edited. A fresh phase is not issued by this package.

## Focused evidence

- Exact actual public config fixture and positive/negative Rust pure-constructor
  probe: old pin fails, new matches, same identity/source under two consumer boot
  IDs; changed deployment generation no longer matches; DB0 remains invalid.
  The probe substitutes only a local filesystem parent, never bootstraps or
  reads a key. It is not an operational restart test.
- 18 Linux/root filesystem unit tests (network-none container), including eight
  before/after atomic-boundary interruptions; apply/resume idempotence, both locks,
  DB15 nonempty, history/root/receipt/key metadata, P0/ELF/install transaction drift,
  symlinks/hardlinks, package mutation and unrecorded/reverted config rejection.
  Systemd/Redis observations are mocked only in these disposable filesystem tests.
- Python manifest-shape cross-check matches the unchanged Rust constructor.
  Existing authority policy/negative checks and package integrity are retained.

The required review is this config-only transition and retained local evidence.
No new architecture stage, Linux release rebuild, broad historical review or
GitHub merge is required merely to prepare it. After accepted stopped correction:
one newly prepared bounded O3 → operational review → O4 bars/WS, reconnect,
freshness/REST overlap/EOD and several ALOR parity sessions. FINAM writes,
runtime-live, DB0 activation and real orders remain closed.
