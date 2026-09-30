# O2 artifact resumption — corrected artifact candidate

Date: 2026-09-28. Status: REVIEW_CANDIDATE_EXECUTION_NOT_AUTHORIZED.

PR #9 required `rust` and `redis-smoke` checks passed in run `36460100665`
at `cfa51fcf5be0187a2a10677dd08d8c280887faa8`. History-preserving merge to
`origin/main` produced `9d9bd1192467532d0ee48350d3c531d9e156dee3`, tree
`0e1ee00d73e23460dc0c4af5a13296d6e2ad12e3`, identical to the accepted PR
head tree. The artifact build is pinned to this merge, not a moving branch.
There are no Rust/Cargo, deployment-unit, workflow or strategy-profile edits
relative to this merge in the artifact candidate.

## Implemented preparation

- Rebind the source and supervisor templates to the accepted baseline07 BO-only
  profile and runtime config fingerprint. No changes to the embedded profile,
  Rust runtime, bars/session windows, trust keys or deployment units.
- Preserve source schema v2: a profile correction does not invent a new schema.
- Add a shared artifact-validation check linking both public templates to the
  exact accepted canonical profile bytes; exercise a positive control and twelve
  mutations (old High180 IDs/hashes/fingerprint, model/MR drift, schemas,
  source sentinel and account alias).
- Include the ten accepted O2 recovery regressions in the existing artifact
  witness commands. No new orchestration or governance framework.

## Build and verification

The existing pinned Docker image built Linux/amd64 release materializer,
operator and paper-supervisor binaries with `--locked`, offline Cargo and
`--network none`. Only exported Git source and public registry cache entered
the build; no workstation checkout, credentials, SSH agent or Docker socket
was mounted. Raw compiler output, recipe, binary hashes and raw build commit
are retained. The handoff reconstructs both the packaging Git tree and the
compiled merge tree; changed original public blobs are retained separately.

The artifact gate covers thirty acceptance rows, 36 contract negatives,
twelve profile mutations and the accepted Rust linked witness, followed by
format and strict targeted Clippy checks. The exact Linux ELF smoke separately
tests custody, capability-free control-root preparation, admission rejection
exit 70, and read-only supervisor config admission/rejection. It has no
network, credentials, authority documents, source bundle or Redis service.
It is NOT execution under a running systemd manager and does not independently
prove OS cgroup stopping; those semantics remain covered by the source tests.
Raw logs, not this description, determine the handoff gate result.

The packager publishes the ZIP only after safety checks and nine isolated
archive mutations pass (payload, build log/commit/manifest/original blob,
smoke coverage, duplicate, symlink and execution authorization). Results are
retained in `.zip.negative.json`; rerun with
`python3 scripts/test_stage8b_p1f_o2_artifact_archive.py ARCHIVE`.

## Bootstrap runtime dependency — requires review

O1 provisioned supervisor SHA-256
`cee324a4e4f251227a25d4a7b23dda332a94522b45407671982f4fc896614406`
from source `940377ab2bd406be31547200ca0b8cc3bb0f3e22`, before the accepted
baseline07 correction. Replacing only the O2 facades cannot update the runtime
actually invoked by the bootstrap unit. This artifact therefore also carries
`stage8b-p1-paper-supervisor` built from the same synchronized merge, SHA-256
`3e336dffa2446f79318fb95a3f6135fa96dd3abeef6ad2dd47494c68b9415f82`.
It is a bootstrap prerequisite, not a sixth O2 role.

The old O1 observation and O2 R1 contract are preserved as historical accepted
bytes. `bootstrap_runtime_prerequisite` explicitly states replacement is
required but NOT authorized. Reviewer must accept this corrected dependency
before a separate non-activating replacement/installation gate; a signed bounded
O2 execution gate follows that. No installed VPS file was inspected or changed
by this local build. O3/O4 are still later roadmap gates.

The old accepted source review and its logs remain immutable. Python template
checks are not Rust runtime execution or Linux unit execution. Docker image
preflight confirmed the existing digest resolves to Linux Rust 1.98.0; this is
the pinned artifact builder, separate from the accepted Mac/CI Rust 1.95.0.
No private inputs are used. VPS, FINAM, operational Redis, signed authority,
service starts, broker writes, O3/O4 and live micro are outside this candidate.
