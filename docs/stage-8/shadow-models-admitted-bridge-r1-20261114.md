# Admitted IMOEXF bridge R1: BR-1, BR-N1, Q-1

Predecessor: e196b0a570fd470bb50482cb6b039f0d83703107, SOURCE CHANGES REQUIRED.
Independent review ZIP SHA256:
4bee3375312c3f945e284e28230538a415988789015114c93cb0a6ec126abd4e.
This is a narrow source correction, NOT an operational data-recovery change.

BR-1: coherent postcommit readback now binds each affected current arm checkpoint
to its manifest/in-memory instrument identity and the actual latest committed
input revision for that instrument. It decodes both latest outcomes, validates
input identity/hash, pair links and chain, checks the arm's latest outcome
revision, and matches current checkpoint/state hashes and serialized state.
Reading an older requested input remains legal after a genuine newer commit.
No causal replay/model callback is added to readback.

Independent reviewer negative cases are adopted as unit regressions. The native
foreign-SQL trigger regression covers both sequence+1 and foreign-instrument
mutations: postcommit Err, producer count stays zero, durable revision stays one,
consumer remains held, reopen rejects the corrupted DB. These are controlled
foreign SQL mutations, NOT claims that the ordinary atomic writer or a real VPS
run produced such damage. Existing atomic SIGKILL guarantees are unchanged.
The independent dense/sparse REST bridge regressions are retained too.

BR-N1: consumer fingerprint includes ROOT Cargo.toml in addition to its own
manifest, root lock and source modules. A workspace-only serde_json feature
change is checked by a synthetic fingerprint test. No toolchain/ELF attestation
claim is added.

Q-1: the audit fixture uses durable state predicates instead of assuming a
second watch notification; already coalesced progress can no longer hang it.
The EOD fixture's final join/drain uses one real anchor to advance BOTH its
fake UTC and fake monotonic samples, avoiding frozen-wall/real-elapsed mismatch.
All clock support is cfg(test), not in the production build. Linux-only
needless_return warnings are fixed using tail expressions in the two
RENAME_NOREPLACE branches, preserving syscalls and error handling.

Original independent R0 full gate failures (490/2 and 499/1) remain historical
FAIL evidence. R1 requires NEW complete native gates and Linux strict clippy;
isolated passing repetitions do not rewrite R0 results. No production timeout,
Pong guard, STOP tolerance or -D warnings is relaxed.

Qualify with the portable source verifier:

    python3 -B scripts/finam_shadow_admitted_bridge_verify.py --output /absolute/new-checks --target-dir /absolute/target

The source review package additionally carries Linux gate records and repeated
Q-1 fixture checks. New unit counts are expected to include the adopted
regressions; exact observed counts are in gate logs, not inferred acceptance.

Numerical kernels/profiles, mapping JSON, strategy parameters, fees, virtual
quantity and feedback stay unchanged. USD/RI native producers, observed model
context, physical bridge restart and continuous sessions are not completed here.
Source authority/governance closure is not asserted. VPS, timers, ELF, DB0,
FINAM order endpoints, main/origin and real orders are untouched.

Low-activity data-tail/reconnect hardening is a SEPARATE subsequent source
slice. This R1 does not alter the accepted V6 delivery/recovery policy.
