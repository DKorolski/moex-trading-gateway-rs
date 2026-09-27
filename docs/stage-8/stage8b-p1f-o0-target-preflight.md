# Stage 8B-P1-f O0 — immutable read-only target preflight

Status: `REVIEW_CANDIDATE_CORRECTION_P1_O001_P1_O002_NO_MUTATION`.

The correction is a direct continuation of held O0 target
`e6b2f2dd2a35145dda4db0b2ae09e0581f56d989`. The HOLD review is
`FINAM_P1F_O0_REVIEW_e6b2f2d_2026-09-27.md`, SHA-256
`5b4e2f04c9427b67857ec564878185b08214a4d56aa2db88fe1160eb8480ce6b`;
it requires only P1-O001 and P1-O002 measurement corrections. The accepted Ie
governance closure remains
`3a46a460ea4bd5c85c5befd036510c580941a265`. The independently accepted Ie
source is `940377ab2bd406be31547200ca0b8cc3bb0f3e22`; its acceptance review is
`FINAM_P1F_IE_SOURCE_ACCEPT_940377a_2026-09-27.md`, SHA-256
`94b224e56c30e4ad54b5db6d0d744b1fd7fbf06897e58af9be3382a3c9d5af96`.

## Scope

The probe performs one SSH read-only observation of the fixed target
`stage8b-p1f-isolated-vps-1` (`45.150.11.252`). It reads host identity,
platform resources, Redis server and client versions, configuration and
keyspace hashes, the two existing P0 unit identities and the complete fixed
installer P1 inventory. The P1 inventory covers service user and group, 17
fixed paths, two regular units, the recovery template and loaded recovery
instances. It does not copy files to
the target, create a temporary file, install a package, create a user or
directory, change Redis, reload/start/stop a service, or invoke FINAM.

The raw evidence is
`reports/stage8b/stage8b-p1f-o0-readonly-probe.txt`; the normalized and
fail-closed result is `stage8b-p1f-o0-target-preflight.json`. DB0 evidence is
hash-only and is an observation, not P1 authority. DB15 is required to be
empty before any O1 mutation. Redis listener endpoints must be exactly the two
loopback endpoints. Redis server version comes only from read-only
`INFO server`; `redis-cli --version` is retained separately as diagnostics and
is never a server-version fallback. Systemd query failure aborts the probe and
cannot be normalized as absence.

## Result

The retained observation proves:

- exact hostname, IPv4 and SSH ED25519 host-key fingerprint;
- Ubuntu 24.04 x86_64, systemd 255, two CPUs, sufficient memory and root disk;
- synchronized time;
- Redis server 7.0.15 from `INFO server`, Redis CLI 7.0.15 as separate
  diagnostics, protected mode and AOF enabled, 16 databases and loopback-only
  listeners;
- DB0 is observed hash-only with six keys and is not modified;
- DB15 is empty;
- both existing P0 FINAM paper units are loaded, active, running and enabled,
  with retained fragment and ExecStart hashes;
- the P1 service user and group and all 17 fixed-install artifacts are absent;
- both regular P1 units are `not-found`/inactive with no fragment, the recovery
  template unit file is not found and no recovery instances are loaded.

All required O0 checks pass. This means only that the target is ready for an
independently reviewed O1 non-activating provisioning package.

## Closed boundary

O0 does not authorize O1. Installation, user/path creation, daemon reload,
service enable/start, Redis DB15/DB0 mutation, paper-provider execution,
FINAM POST/DELETE, broker dispatch, runtime-live and real orders remain closed.
