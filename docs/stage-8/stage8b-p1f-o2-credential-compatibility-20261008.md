# O2 lifecycle credential compatibility — October 8

## Scope and proportionality

The owner requested the narrow correction and explicitly asked to keep the
development/paper path proportionate. Do not add a key ceremony, another VPS,
general recovery machinery or a new authorization framework for this mismatch.
FINAM read-only token permissions and broker execution permissions are unrelated.
No fallback/test key, root bootstrap execution or unconditional permission bypass
is introduced. This note records a source correction, not O2 acceptance.

Baseline: `332ead33b14754d73f8ab3678d963022ce485f47`. The preceding bounded
attempt reached bootstrap and failed with `lifecycle credential is invalid`.
It remains sealed as FAILED / generation 1 / sequence 19. Its history, retained
inputs and installed payload must not be reset or silently updated.

## Cause and minimal fix

The old loader required `mode & 0077 == 0`. A native synthetic `LoadCredential`
probe under the installed bootstrap's service UID and sandbox instead observed
a root:root 0440 regular file, a root:root 0550 directory, a POSIX access ACL and
a read-only mount. The process could open the file; the application's metadata
guard rejected it. The synthetic probe did not read the real lifecycle key.

This fits the upstream [systemd credential delivery contract](https://github.com/systemd/systemd/blob/main/docs/CREDENTIALS.md):
the service manager delivers a separate read-only copy accessible to the service.
Group mode bits can reflect the ACL mask, rather than ordinary group access.

Only the lifecycle loader changes. Private regular 0400/0600 files remain
supported. A 0440 file is additionally accepted only when it is root:root and
located directly inside a canonical root:root 0500/0510/0550 directory under
`/run/credentials`, with `fstatvfs` proving the opened file's mount is read-only.
The system service manager remains the trusted owner of that directory and its
ACL policy; this does not introduce an application-level ACL engine. This
compatibility exception is Linux-only. Arbitrary group-readable files do not
qualify. Other bits, including write/execute/special bits, do not qualify.

Existing checks remain: canonical absolute directory, no symlink file open,
regular file, single hardlink, exactly 32 bytes, root/effective-UID ownership,
exact read and clearing of the temporary secret buffer. The on-disk source key
is not chmod'ed, copied into a user-controlled directory or reissued.

## Qualification

- Unit tests: private modes; ordinary 0440/0640/0644/0660/0666, executable and
  special-bit rejection; short/long/empty key, directory, symlink and hardlink.
- Native probe: `scripts/stage8b_p1_systemd_credential_probe.py` calls the actual
  Rust loader via `tests/stage8b_systemd_credential.rs`, linked to the production
  release library without fixture features. Public `x` bytes only.
- Positive native `LoadCredential` under the installed bootstrap service UID;
  negative 31/33-byte credentials, ordinary read-only 0440 outside the systemd
  namespace, and a writable fake directory inside that namespace.
- Native transient services do not run bootstrap, guardian, FINAM or Redis.
  They omit durable write access; production units and payload are not changed.
- Retain command logs and explicit before/after state. Do not call a synthetic
  loader PASS a successful O2 attempt. Broader test results belong in the report.

Reproduce the native probe on the existing qualified Linux systemd host (test
binary built for that host, not the Mac). No fixture-feature build is required:

```sh
cargo build --locked --release -p runtime-durable-service --lib
rustc --edition=2021 --test crates/runtime-durable-service/tests/stage8b_systemd_credential.rs \
  --extern runtime_durable_service=target/release/libruntime_durable_service.rlib \
  -L dependency=target/release/deps -o target/credential-probe
sudo python3 -B scripts/stage8b_p1_systemd_credential_probe.py \
  --test-binary /opt/public-test/credential-probe \
  --unit /etc/systemd/system/moex-finam-p1-paper-bootstrap.service
```

Place only the test executable in the temporary, executable `/opt/public-test`
directory; `ProtectHome`/`PrivateTmp` intentionally prevent using a home/tmp
binary inside the service. Remove the temporary test copy after qualification.
The probe generates its own public bytes and never requires a real credential.

Run the library regression with `--all-features --lib -- --test-threads=1`, as
already required by `scripts/stage8b_p1f_source_gate.sh`. The guardian tests share
process-global fault injection; the default parallel test run is not that gate.
Do not change the guardian/test framework as part of the credential correction.

## Next operational boundary

Unlike the earlier boot-ID unit fix, this correction changes Rust and therefore
requires a rebuilt supervisor and updated artifact/installation binding. Reuse
the existing build/install path; preserve terminal history and the current P0.
Do not start an installed old binary and expect this source change to apply.
The source/native proof is the next review boundary. After its review and the
updated artifact installation/preflight, use one newly authorized bounded O2,
fresh inputs and a fresh phase; no retries, no replay of the FAILED19 phase.
Continuous paper, FINAM order POST/DELETE, broker dispatch, runtime-live and
real orders remain closed.
