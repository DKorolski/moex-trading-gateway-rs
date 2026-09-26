# Stage 8B-P1-f Ie — aggregate source closure

Status: `REVIEW_CANDIDATE_AGGREGATE_SOURCE_CLOSURE_NO_ACTIVATION`.

P1F-Ie closes the already implemented source composition. It adds no
production Rust, Cargo, Redis, FINAM, installation or service-start surface.
The immutable predecessor is the independently accepted P1F-Id R2 source at
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

The commits form one ancestor chain. The Ie candidate permits only this
document/evidence/gate surface over the accepted Id tree, so accepted
production and Cargo bytes cannot drift inside aggregate closure.

## Linked local composition proof

`scripts/stage8b_p1f_ie_linked_local_composition.sh` executes nine existing
behavioral fixtures in dependency order:

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

This is a linked local composition proof over accepted fixtures, not a claim
that one monolithic test process models the later VPS deployment. It neither
uses operational DB15 nor contacts FINAM. The Redis fixtures create isolated
ephemeral local servers only.

## Aggregate gate

The Ie gate validates the authority summary and its mutation harness, runs the
linked proof, then reruns the complete runtime and FINAM suites, doctests,
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
