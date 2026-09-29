# O2 old-phase terminal recovery — authority/artifact preparation

Date: 2026-09-29. Status: PREPARATION / INDEPENDENT REVIEW REQUIRED.
This document is not operational permission. No VPS, FINAM, operational Redis,
new claim, signing, install, service mutation or terminalization is performed by
the package builder. No main merge or successful GitHub CI is asserted here.

## Accepted boundary and authority delta

Source `590304af44830197704503c8ebc67329693ac75b`, tree
`133a375e30491a0897dbc85d2d5a50ca9061fee5`, ZIP SHA-256
`ca0fe37980d5ea69d3cd95fe2951664b1441f2e8344cd005b325183afd04c485`
is independently SOURCE ACCEPTED. Review
`FINAM_590304a_O2_RECOVERY_SOURCE_REVIEW_2026-09-29.md` SHA-256:
`4ff21077154d17b2f911fa2919dba9f9a2adfc8b6ba48f6979eda0ed51da26fe`.

The successor changes only documentation, preparation scripts and current-tree
authority inventory. Rust/Cargo, strategy, trust keys, units and CI policy remain
byte-identical to accepted source. The production inventory rebind covers exactly
its five Rust changes; the existing control inventory refresh changes only the
two status documents. No closed-surface flag or historical replay pin is changed.
Local authority/negative checks are not substitutes for fresh required GitHub
`rust` and `redis-smoke` checks at the proposed PR revision. No self-acceptance.

## One artifact, no installed-byte substitution

`scripts/stage8b_p1f_o2_terminal_recovery_package.py` reuses the existing pinned
Linux builder and commit/tree-bound ZIP verifier. It builds **only**
`stage8b-p1f-o2-operator` from a clean exact preparation commit, after proving no
production delta against accepted `590304a`. Source is exported from Git, not
mounted from the working copy. Network is disabled; only public Cargo registry
cache is copied. The pinned Rust image, toolchain output, locked/default-feature
Cargo command, ELF x86-64 identity, binary size/hash and build log are retained.
No `--all-features`: the known baseline-only FINAM legacy one-shot test conflict
remains documented, not fixed or mislabeled PASS. The new operator has its
existing multi-command CLI; this package authorizes no other CLI operation.

The Linux witness uses the exported source and same offline toolchain for the
16 O2 release tests. These use the real guardian store but substitute systemd
observations and clock. An actual ELF invocation with no arguments must exit 70
with usage in a networkless non-root container, proving loader/CLI compatibility
only. No real systemd manager, production UID custody or VPS execution is claimed.
Native manager/stopped-proof evidence is collected at the separately authorized
operation. Materializer read-only-directory/lock-inode mount qualification belongs
to the **next full O2 artifact**, not this operator-only delivery.

Build and preparation evidence stay outside Git, included in the immutable ZIP
with source acceptance and full source-gate package. Build hashes are generated
only after the real build; `handoff-evidence/build.json` is the exact artifact
descriptor, not a template. The ZIP SHA and safety sidecars bind the package.

## Target and old phase (retained, not fresh observations)

- Target: `stage8b-p1f-isolated-vps-1`, `45.150.11.252`.
- SSH Ed25519: `SHA256:8fOAPkfvZ61LYmldUBs6A5ZC+BFr6O3yCvUrBAWZsAo`.
- Existing installation ID: `stage8b-p1f-o2-baseline07-7196aaa-v1`.
- Installation SHA-256: `316c2376cf4d02a7f0ee3837e96d93bbf2cb1b2b8e3aabe10205f783f088aad8`.
- Manifest: `b8ce96287e82a1268a3617c3e29bf8ec0167014bd216c544011c07b709e3b3b8`.
- Generation 1; last observed sequence 1 / Active, no pending terminal.
- Deadline: `2026-09-29T14:49:46Z`.
- Last retained P0 unchanged, DB15 empty, no P1 process; not a current health claim.

## Delivery and preflight — after review and explicit permission only

Reserve an exclusive root maintenance window: no concurrent operator, timer,
runner, activation or deployment. Do not stop P0. Obtain fresh stopped proof
for ordinary P1, bootstrap, materializer, runner and recovery instances; include
full unit inventory/drop-ins/jobs, all P1 service-UID/root executable processes
and descendant cgroups. Incomplete observation or changed P0 stops the operation.

On the verified target stage the reviewed package under a **new** root:root 0700
directory `/root/moex-o2-terminal-recovery-590304a`. Refuse any pre-existing path
or symlink component. Check received ZIP SHA against the reviewer-approved value
before safe extraction (CRC/duplicates/path/mode/exact Git binding verifier).
The operator is `handoff-evidence/stage8b-p1f-o2-operator` inside that directory:
compare bytes with build.json, root:root, single regular link, chmod 0500. Do not
copy it to `/usr/local/libexec/moex/`, edit any installed unit or daemon-reload.
Record staged executable identity separately from the unchanged installation.

Recheck the existing installation manifest and every installed file's hash/mode/
owner/link count against its accepted inventory; save before/after checksums.
Do **not** call the old installer `preflight/install/resume/rollback`: it requires
an empty pre-activation root, which is intentionally no longer true. Reuse only
read-only observation logic if needed, never its transaction entry points.

Read and retain bounded local evidence, without dumping credentials: fixed
selector, exact manifest/claim/genesis binding, history-head/events hashes,
pending-terminal/pending-stopping presence, materialization files and owner.
For the originally observed branch require Active generation 1 / sequence 1,
the exact expired manifest, no Ready/source/config/partial publication/owner.
Unexpected frontier/corruption => STOP, no pruning or repair by guessing.
If an exact pending terminal exists, preserve its original decision and stop
to select the reviewed exact-replay branch; do not force Expired/sequence 2.
`guardian-inspect` may reject an event-ahead-of-head pending frontier: that is
not authority to delete pending state. Review retained bytes before continuation.

## Bounded invocation proposal — NOT RUN by preparation

After all preconditions and explicit permission, use the separately staged
executable. This transient recovery unit is not the original runner: it has no
ExecStopPost, does not start bootstrap and is never stopped by cleanup itself.
No environment file, credentials or materializer invocation is used.

```sh
systemd-run --unit=moex-o2-terminal-recovery-590304a --wait --pipe \
  --property=Type=oneshot --property=User=root --property=Group=moex-p1-paper \
  --property=UMask=0027 --property=TimeoutStartSec=180s \
  --property=TimeoutStopSec=15s --property=KillMode=control-group \
  --property=PrivateNetwork=yes --property=RestrictAddressFamilies=AF_UNIX \
  --property=NoNewPrivileges=yes --property=CapabilityBoundingSet= \
  --property=ProtectSystem=strict --property=PrivateTmp=yes \
  --property=ProtectControlGroups=yes --property=LimitCORE=0 \
  --property=ReadWritePaths=/var/lib/moex-finam-p1-paper-control \
  /root/moex-o2-terminal-recovery-590304a/handoff-evidence/stage8b-p1f-o2-operator cleanup-fixed
```

Capture exact command, executable SHA, times, stdout/stderr and exit status.
Cleanup can issue bounded stop/kill for bootstrap and materializer only, and
validates both stopped proofs while holding the execution lock through commit
and reread. It does not create a new run permit. Timeout/error is NOT success;
retain all files and obtain fresh observations before considering an exact replay.
Do not `reset-failed`, delete the transient unit evidence or change deadlines.

## Postconditions before any new O2

For the unchanged original frontier require cleanup exit 0 and exact terminal
receipt: manifest above, generation 1, sequence 2, `Expired`, reason
`o2-materialization-incomplete`. Preserve its recorded timestamp; never generate
a replacement in a report. Compare returned receipt to the durable receipt bytes
and validated history, then collect a separate read-only `guardian-inspect`.
For an exact pending-terminal branch compare all retained receipt fields instead.
Already-terminal replay must return the identical receipt, no new event/sequence.

Require no pending terminal, no new claim/genesis/activation, no new Ready/source/
config/owner/bootstrap root, no P1 PID/job/populated cgroup, unchanged installation
and selector, unchanged P0 identities. The only intended control-state changes
are terminal transaction/history/receipt and existing lock use. Save evidence
before removing nothing; operational evidence excludes private/signed documents.
The observed old BarsTruth cause remains unknown.

Send actual terminal evidence for review. Only after acceptance prepare updated
full O2 artifact/installation binding preserving the terminal history, qualify
the materializer lock mounts on Linux/systemd and request a fresh bounded O2.
Do not reuse the empty-root installer or silently relabel this recovery as new
installation. O3/O4 paper/ALOR sessions follow; FINAM writes/live stay closed.

## Local reproduction

From the clean committed successor (Docker pinned image must already exist):

```sh
python3 scripts/stage8b_p1f_o2_terminal_recovery_package.py build NEW_BUILD_DIR --registry-cache /absolute/public/cargo/registry
python3 scripts/stage8b_p1f_o2_terminal_recovery_package.py gate BUILD_DIR --review /absolute/FINAM_590304a_O2_RECOVERY_SOURCE_REVIEW_2026-09-29.md --accepted-source-zip /absolute/moex-trading-project-590304a-o2-recovery-review.zip
python3 scripts/stage8b_p1f_o2_terminal_recovery_package.py package BUILD_DIR
python3 scripts/stage8b_p1f_o2_terminal_recovery_package.py check REVIEW_ZIP
```

Fresh GitHub checks and their exact head SHA are reported separately, never
inferred from local gate logs or the old green main run. Package acceptance,
merge and target execution are distinct decisions.
