# O2 installation R1 — native not-found property correction

Date: 2026-09-29. Status: LOCAL_SOURCE_CORRECTION_REVIEW_REQUIRED.

## Accepted package and bounded operational attempt

Installation package `304cd56bd33e2145f327c5f1ea02f56837cc3e62` was independently
accepted. Review SHA-256:
`20b48c7ad8a7a0d16f279eb11f663b078093d1bfce4e5a9a658e079ddcb2dec9`.
The user subsequently authorized fresh read-only preflight and installation
only if its prerequisites passed, without activation or force/cleanup fallback.

Fresh host/platform/P0/DB15 and existing O1 bytes/custody checks passed. The exact
accepted ZIP was copied into root-only `/root/stage8b-p1f-o2-install-304cd56` and
its full checksum/tree/safety verification passed again on the target.
The immutable installer was invoked only with `preflight`, returning exit 1:

```text
stage8b-p1f-o2-install: FAIL incomplete systemd response
```

No `install`, `resume`, `rollback`, daemon-reload, enable/start, signing, genesis
or bootstrap command followed. The immutable package and staged installer were
not edited. After the stop, read-only verification with the accepted helper
confirmed exact O1 managed bytes, empty durable state, no replacement transaction
and no new installation manifest. P0 identity/running state stayed unchanged;
DB15 stayed empty. DB0 was read for hash-only observation, never mutated; its
data digest need not remain constant while P0 runs. Staging is retained, not
silently removed or reused as a different package.

## Cause and minimal correction

On the real target (systemd 255), both new O2 units are legitimately absent.
`systemctl show` returns exit 0 with nine requested properties, omitting the
service-specific `ExecStart`. Adding `--all` produces the same nine fields.
The 304cd56 test double supplied ten fields, so this shape was not covered.
This is an installer observation-contract bug, not a defective VPS or evidence
of a running P1 service.

`properties()` now allows exactly this one missing field only for the two fixed
new O2 unit names and only with all these returned facts:

- `LoadState=not-found`, `ActiveState=inactive`, `SubState=dead`;
- MainPID and ControlPID both zero; no pending Job;
- empty FragmentPath, DropInPaths and ControlGroup;
- all other requested fields present exactly once, with no extra/malformed rows;
- a successful observation command, not a failed query interpreted as absence.

No ExecStart value is synthesized. Loaded units, P0, old P1 units and unknown
unit names still require the complete original property inventory. Missing other
fields, duplicate properties, active/failed/unknown states, PIDs, jobs, fragments,
drop-ins and cgroups reject the nine-field special case. Independent exact disk
inventory, bytes/custody, service-UID/root process and cgroup checks are retained.
The same stopped not-found observation may persist after copying new unit files
without daemon-reload; this is not a claim of manager reload or runtime readiness.

## Evidence, tests and identity

`scripts/fixtures/stage8b-o2-systemd255-not-found.json` is a byte-for-byte copy of
the captured public response fixture, not a reconstructed successful query.
Supporting raw outputs, read-only diagnostic programs and the previous acceptance
review are retained under `docs/stage-8/evidence/o2-installation-r1/`.
The offline `stage8b_p1f_o2_install_r1_evidence_check.py` checks these records as
part of the package gate. The new candidate's `target_mutation_performed=false`
refers to R1 preparation, not to the parent attempt: that attempt did create
root-only staging, as explicitly recorded above. No corrected code was deployed.

The suite grows from 15 to 17 methods: exact native response parsing/rejections,
and composed observer replay with absent units before/after disk copy. Existing
12 durable-file interruption frontiers and all rollback/custody tests remain.
The 45 authority mutations remain inherited governance regression checks.
Fixture replay is not a deployment or an actual systemd lifecycle test.

The canonical inventory binds the installer source hash, so this correction
changes installation identity SHA-256 to
`316c2376cf4d02a7f0ee3837e96d93bbf2cb1b2b8e3aabe10205f783f088aad8`.
The artifact-oriented label remains `stage8b-p1f-o2-baseline07-7196aaa-v1`.
The earlier `7252b88c…` identity was never installed and no transaction exists;
do not relabel an old receipt or sign against the superseded hash. All binary,
unit, public-template, Rust/Cargo, profile, workflow and custody-helper bytes
remain exactly the accepted artifact bytes.

## Next boundary

Review this narrow correction, then confirm authority for a fresh preflight and
non-activating install from its own immutable staging path. Do not patch or force
the existing 304cd56 staging. After actual installation evidence is accepted,
bounded O2 still requires separate authorization. No new deployment framework,
VPS replacement or historical re-audit is needed.
