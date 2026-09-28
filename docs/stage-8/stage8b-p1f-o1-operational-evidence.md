# Stage 8B-P1-f O1 — non-activating provisioning operational evidence

Status: **R1 CORRECTION REVIEW CANDIDATE**.

The immutable O1 package at
`8864a2bbba64ef930073fae4e71dfcde82ceba58` was independently accepted by
`FINAM_P1F_O1_PACKAGE_ACCEPT_8864a2b_2026-09-27.md` with SHA-256
`b3faa0eaceca62b2c7991791cdb348e0530359466b7aca29e9da4dafd8ae16e8`.
The first operational evidence at
`645cb3555ca410f5a00795830ade896e89887e5b` was held for P1-O1E01 and
P1-O1E02. This correction records only the authorized non-activating
installation on `stage8b-p1f-isolated-vps-1`.

P1-O1E01 is closed in the candidate by one deterministic verification path:
the retained pre-O0 raw bytes are parsed and rebuilt by the accepted O0
normalizer, the retained post-install raw bytes are parsed and rebuilt by the
O1 normalizer, and the complete rebuilt object must equal the committed JSON.
No retained PASS flag is trusted independently. Semantic mutations of binary
hash, directory mode, P0 fingerprint and historical P1 absence are rejected
after their raw SHA-256 fields are recomputed.

P1-O1E02 is closed in the candidate by status-checked systemd queries and
exact state classification. The two regular units must be
`loaded/inactive/static` with exact FragmentPath. The recovery template is
classified separately as `not-applicable/static` with exact identity, while
the recovery-instance inventory must remain empty. Empty/error output,
`enabled-runtime`, active state and query failure are rejected.

## Executed sequence

Immediately before the first mutation, a fresh complete O0 observation passed
at `2026-09-27T08:48:21Z`. It proved the exact target and P0 identities, absent
P1 identity/material and empty Redis DB15. The accepted nested bundle SHA-256
`f90fea1357a0f959d119027ef07becd4e1175995223037c83ed9e70c93db73c1`
was copied to the new root-owned `0700` staging directory
`/root/stage8b-p1f-o1-8864a2b` and verified before extraction.

The accepted fixed package was installed once with the canonical absolute bundle
and binary paths. It exited zero and returned `INSTALLED_OR_ALREADY_EXACT`.
The surrounding operator assertion then returned one because it expected an
incorrect textual result label. That assertion ran after the installer had
completed; it caused no second install or other remote action. The independent
status reread returned `EXACT_INSTALLED`.

The corrected post-install read-only observation at `2026-09-27T09:20:58Z`
proves:

- all six managed payloads have the accepted SHA-256, root ownership, exact
  mode and one hard link;
- the canonical installation manifest is root-owned `0644`, and all of its
  activation, daemon-reload, Redis, FINAM and operator-material flags are
  `false`;
- the dedicated service user/group and all six persistent directories have
  exact custody;
- operator config, first-boot source, lifecycle credential, initialized
  durable state, P1 processes and recovery instances are absent;
- all three P1 units are not active and are not enabled;
- both P0 services remain active with byte-identical unit and ExecStart
  fingerprints;
- Redis DB15 remains empty.

DB0 contained six keys before and after installation, but its digest changed
while P0 remained active. Full DB0 equality is deliberately not an acceptance
condition. The accepted installer manifest independently states
`redis_contact_performed=false`; the O1 sequence issued no Redis mutation.

## Retained evidence

- `docs/stage-8/stage8b-p1f-o1-operational-evidence.json` is the normalized
  evidence and check result;
- `reports/stage8b/stage8b-p1f-o1-pre-o0-readonly-probe.txt` is the exact fresh
  pre-mutation O0 output;
- `reports/stage8b/stage8b-p1f-o1-post-install-readonly-probe.txt` is the exact
  post-install read-only output.

## Closed boundary

No reinstall, rollback, `systemctl daemon-reload`, enable or start was
performed for this correction. O2 bootstrap,
Redis mutation by O1, paper-provider execution, FINAM POST/DELETE, broker
dispatch, runtime-live and real orders remain closed. O1 does not authorize O2.
A separately reviewed fresh-materialization and network-isolated one-shot
bootstrap package is required next.
