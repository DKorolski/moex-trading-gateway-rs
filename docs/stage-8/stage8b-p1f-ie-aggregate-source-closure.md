# Stage 8B-P1-f Ie — aggregate source closure

Status: `REVIEW_CANDIDATE_LINKED_COMPOSITION_CORRECTION_NO_ACTIVATION`.

P1F-Ie closes the already implemented source composition. The initial Ie
candidate `0394cb702e7d11e1ee80f5a2f18177906bcc6be5` was held because nine
independent regressions did not prove a connected execution. The correction
authority is `FINAM_P1F_IE_REVIEW_0394cb7_2026-09-26.md`, SHA-256
`99618000f046a5ff1b399bce720029af3a6fecd35d4929e8fdfbdca5f2322cea`.
This correction adds one true isolated local composition witness and only the
feature-gated fixture seams needed to connect accepted components. It adds no
Cargo, deployment, Redis-operational, FINAM, installation or service-start
surface. The accepted source predecessor remains P1F-Id R2 at
`512db6e6e652a2b0a15be7b6dcb72b96e231950d`.

## Accepted authority chain

The aggregate authority file binds all four accepted source slices and their
independent review digests:

| Slice | Accepted commit | Independent review SHA-256 |
| --- | --- | --- |
| P1F-Ia guardian foundation | `9be356b04a38e627337ed148ccc9fbdaebae8d4a` | `db684cb6ba10cb951801cc917c317d2817c99c3f10874d5a1d959c053f6ebb60` |
| P1F-Ib local supervision | `7c481bc60699b514b016e8dffe62eb9ca462a100` | `098a24fc87871968bc6e7b0a77deefffd403bc56d997d21af18e1869750acf7e` |
| P1F-Ic fixed producers | `5c2656fbe8691da256b5380dd16ce6f6b6aa1fa8` | `ee69d58bc70288f447a9ab880d2a2eec01fefdfe6f86ce3be603eb7f5a1b30b3` |
| P1F-Id fixed Redis composition | `512db6e6e652a2b0a15be7b6dcb72b96e231950d` | `3208f8472f7e9a0109f41187458f78bfdab780029b4cc6be610640e5c652b1b9` |

The commits form one ancestor chain. The correction has an exact changed-path
allowlist. Thirteen existing Rust files receive fixture-feature seams; no Cargo
file or new production Rust file is added. The source checker pins both the
accepted Id ancestry and the held Ie review target.

## Linked local composition proof

`scripts/stage8b_p1f_ie_composition_witness.sh` runs one connected witness. In
that execution:

1. the guardian materializes actual O2 supervisor/source bytes and the runtime
   bootstraps and admits from those exact files;
2. the fixed schedule and M10 producers publish their retained bytes into one
   ephemeral Redis read by the supervised paper child;
3. operational identity, runtime fingerprint, source/config hashes, schedule
   Redis ID, both M10 Redis IDs and producer sequence/high-water remain linked;
4. the existing local supervisor owns the participating child while resource
   polling and bounded hash-only audit observe the same Redis execution;
5. one Market lifecycle reaches durable ACK, durable truth and XACK-last;
6. restart/readmission from the same durable root returns
   `AlreadyAcknowledged`, keeps command stream length one and emits no duplicate
   command/effect;
7. a corrupted materialized source-byte control is rejected before first boot.

The redacted `STAGE8B_P1F_IE_LINKED_EVIDENCE` record carries those links and
explicitly records `operational=false`. The witness uses no VPS, operational
DB15/DB0 or FINAM endpoint.

The nine inherited fixtures are an aggregate regression suite, retained in
`scripts/stage8b_p1f_ie_linked_local_composition.sh`:

1. Ia materializes and rereads the exact O2 source/config set;
2. Ib admits the fixed local child and completes bounded signal supervision;
3. Ic advances one retained O3/O4 schedule sequence;
4. Ic persists `Prepared`, performs the real isolated-Redis M10 effect and
   exact reread, restarts before `Published`, and converges to one retained
   entry and one `Published` high-water;
5. Id performs bounded real-Redis resource reads and hash-only audit;
6. the production runtime recovers an exact pending M10 with audited
   `XAUTOCLAIM`;
7. the production process retains failed attach audit at terminal output;
8. it also retains accumulated audit after bounded owner abort;
9. the accepted V5 production fixture runs the continuous market lifecycle
   and exact readmission path.

They are not presented as composition evidence. Both the connected witness
and every aggregate regression run through
`scripts/stage8b_p1f_ie_exact_test.sh`, which requires one occurrence of the
full qualified selector and a Cargo result with one passed, zero failed and
zero ignored tests. The negative harness proves that an exit-zero
zero-selection transcript and a qualified-selector mutation fail closed.

## Aggregate gate

The Ie gate validates the authority summary and its mutation harness, runs the
connected witness and aggregate regressions, then reruns the complete runtime
and FINAM suites, doctests,
strict all-target/all-feature Clippy, formatting and diff hygiene. Its retained
log and exact exit status are embedded in one immutable handoff together with
all four accepted review documents.

## Closed boundary

Ie acceptance closes only P1F source composition. It does not authorize:

- P1F-O0 read-only target preflight;
- installation, systemd activation or VPS mutation;
- operational Redis DB15 or DB0 access;
- paper-provider execution;
- FINAM POST/DELETE or broker dispatch;
- runtime-live or real orders.

After independent acceptance of Ie, the next separately authorized boundary
is P1F-O0 immutable read-only target preflight. Every later operational phase
retains its own immutable acceptance gate.
