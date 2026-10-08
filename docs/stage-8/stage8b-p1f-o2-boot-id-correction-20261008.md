# O2 October-8 boot-ID sandbox correction

Owner requested correction and one new bounded O2 after the 09:50 MSK failure.
This is a narrow operational correction, not independent acceptance of O2.
Baseline installation: `2edb1cd5cacdeeef74c8d03dddf47b5699104e82`; compiled
ELFs remain exact `21cc7eb9530ccc4480bae2a4f8264705a7383c01`.

## Evidence and cause

The one-shot caller ran at 09:50:19, authentication/claim/materialization passed,
and source v4 contained 420 historical M10 bars plus the closed 09:40–09:50
candidate. Admission failed at 09:50:24 with authority `NotFound`, before the
bootstrap service started. Cleanup sealed FAILED / generation1 / sequence15.
Head digest: `645ad005c1ce7c6686facfd5ba160f73a91f89b3801d5a5653a3f33911e2c6a6`.

Production guardian `current_boot_id()` reads `/proc/sys/kernel/random/boot_id`.
The runner's `ProcSubset=pid` hides that path. An isolated native proc mount and
then real transient systemd probes reproduced ENOENT for `pid` and successful
reading for `all`. The latter also proved kernel-path read-only mount flags,
empty effective/bounding capabilities, NoNewPrivs and refused AF_INET sockets.
The probe runs Python reads, not a guardian/claim/bootstrap operation. Tests
cannot claim actual corrected O2 success; that is a separate bounded result.

This behavior is described by the upstream [systemd execution contract](https://github.com/systemd/systemd/blob/main/man/systemd.exec.xml).
The source test-fixture fallback for missing boot ID is not production behavior
and did not cover this real unit mismatch. No fallback is enabled or added here.

## Minimal correction and explicit trade-off

Only the O2 bootstrap runner changes `ProcSubset=pid` to `all`. This exposes
non-PID proc information inside that unit; it is not claimed to expose only one
file. It avoids a new boot-ID credential/export mechanism or binary rebuild.
ProtectKernelTunables, ProtectProc, PrivateNetwork, AF_UNIX restriction, empty
capabilities and all other sandbox/command settings remain exact. Other units,
Rust/Cargo, FINAM permissions and CI/governance are untouched.

The correction installer changes only runner unit and installation-o2 manifest.
Manifest pins the new unit bytes, retains exact accepted ELF artifact lineage,
and records predecessor `309ab4fc557f44dcbc56cc081bb32b125abd2dc8470c3062faa88421d20b2974`.
Before-images and journal are root-only under
`/usr/local/share/moex/stage8b-p1e/o2-boot-id-after-failed-15-20261008`.
Existing guardian/execution locks and accepted fsync/atomic-write primitives are
reused. The installer does not reload systemd or start services.

The failed attempt's source and final config are receipt-bound and retained by
rename, along with the unused root0400 lifecycle key, into that root0700
transaction. The key is not read/exported into evidence. Its original metadata
is retained. Authority history/receipts/archives, staged diagnostics, old
installation transactions and exhausted timers remain unchanged. Durable
bootstrap state is still uninitialized. No reset, timestamp refresh or stale
source reuse is allowed. Partial transactions fail closed for operator inspection;
there is no automatic resume/retry/rollback in this narrow installer.

## Qualification and repeat boundary

`stage8b_p1f_o2_boot_id_check.py` checks exact unit policy; its explicit native
mode runs collected transient read-only probe units, never an O2 command.
Contract mutations and disposable Linux filesystem transaction tests cover
foreign source/history/payload, repeated apply, and retained partial writes.

After controlled installation/reload: verify exact payload and stopped P1,
unchanged P0/FAILED15/history and retired inputs; prepare a NEW signed phase at
sequence16, bound to the corrected manifest, unchanged October-8 calendar and
fresh FINAM source/truth. One separately recorded bounded attempt is permitted
by the owner's current request. No expired phase is extended or replayed.

If any new failure occurs, record it and stop; do not create a retry loop.
O2 remains HOLD until its actual result is evaluated. O3/O4/continuous paper,
FINAM order POST/DELETE, broker dispatch, runtime-live and real orders remain closed.
# Bootstrap-chain completion before the retry

The stopped downstream `moex-finam-p1-paper-bootstrap.service` has the same
conflict: `load_validated_supervisor()` reads the real boot ID before bootstrap.
The October-8 failure never reached that unit. Before issuing the successor
phase, apply the same `ProcSubset=all` correction there, retaining its service
UID, lifecycle credential, read/write paths and all other hardening. No ELF or
strategy change is needed. This is a predicted failure prevented before retry,
not a second failed operational attempt.

Native read probes run both original/corrected proc policies as the production
service user. They omit `LoadCredential` because no key is needed for a read
probe; the production credential contract is still checked and unchanged.
Retain the first runner-only installation transaction; the bootstrap follow-up
has its own two-file transaction and manifest predecessor. No history reset,
automatic retry, P0 change, real order or live runtime is authorized.
