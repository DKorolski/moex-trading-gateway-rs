# Stage 8B-P R2B Generation-2 Full Transaction Rebind / Native Proof R0-R1

## Status

The Generation-2 Composition Rebuild R0-R1 at
`c74382a7e3a63d3673dec220ff4e9caaba6b48ee` is accepted. This stage rebinds
the complete 31-service transaction and prepares its native Linux/amd64
controlled-installation proof. It does not activate Generation 2, issue R2B,
install production credentials, contact FINAM or authorize a production-host
installation.

The corrected native runner is implemented but has not been executed. Its source,
proof-tool hashes and fail-closed pre-container boundary require independent
review before the temporary Generation-2 ceremony may be projected into the
disposable container.

## Additive rebind

The accepted Generation-1 transaction contract remains immutable. The new
Generation-2 contract is additive and preserves the same six phases, 31 exact
service invocations, 18 production unit/target files and one proof-only
trigger.

The six Phase-1/2 executables are inherited byte-for-byte from the accepted
upstream build. The six executable roles used by Phases 3-6 are rebound to the
accepted Generation-2 build:

| Role | Generation-2 SHA-256 |
|---|---|
| authority producer | `fa494d0150cb3ed0f5f05378a8e1636f3160499f9f5cc881cbbed862c96229fc` |
| authority issuer | `6dc5be078029a833b2e465525498c76e8d5966fa2c8d4733cfa3dce6b5af74e0` |
| run-package draft builder | `f171fc282e56d509e30bb92ea40340e559b19dc12ac63f9513bed9a926b72207` |
| package issuer | `5aff3f7d4747113546272cb40fc444b5bfa0013116b49d20669e8e757091625c` |
| launcher | `52dfbd0e6bb0d07a92a3104be50c33a60af08905b6cd075aa4bd4a4c373da17e` |
| read-only helper | `90508e097c8668d6fe90a15ef6014e480a9042bb36f0613351c02465d10aaca1` |

No production ELF is rebuilt by this rebind stage.

## Native host boundary

The execution proof requires a disposable x86_64 Linux host whose native
kernel and Docker daemon both report `x86_64`/`amd64`. QEMU, Rosetta and
binfmt emulation are not accepted as native evidence. A host carrying broker
credentials, a production account contour or another sensitive trading
workload is ineligible even when the proof container uses `--network none`.

The known broker/runtime VPS is therefore not an eligible proof host. Docker
Desktop on the ARM developer workstation may be used for syntax and
fail-closed preflight only, never for the native-execution claim.

A separate clean Ubuntu 24.04 Linux/amd64 VPS has now been identified and
attested as the intended disposable proof host. It contains no broker
credentials, trading workloads or production account contour. Identification
of the host does not itself authorize container creation.

The runner starts from the actual reviewed ZIP, recomputes its accepted digest,
checks the complete source manifest and generated-member inventory, and only
then executes the runner from a fresh extraction on host tmpfs. It does not
depend on `.git` metadata. Before container creation the runner must prove:

- a fresh extraction of the reviewed handoff;
- exact source commit, source tree and archive SHA-256;
- exact hashes for all 12 production ELF and 19 unit files;
- native x86_64 host and daemon architecture;
- absence of a sensitive-host marker and explicit disposable-host attestation;
- no default route or DNS in the future container;
- matching Generation-2 offline ceremony fingerprints without printing or
  persisting private paths or values.
- exact privileged image ID
  `sha256:3cc66c640df0444530a626d2acbcfeda9742039b917a747fd023b315ef2c1526`.

Any missing precondition aborts before container creation.

The outer runner invokes that preflight before `docker create`, launches by
the immutable image ID, then uses `--network none`, a tmpfs `/run`, and a
separate tmpfs ceremony projection. A hash-pinned Linux/amd64 Rust verifier
derives all Ed25519 public keys in memory, exact-checks the ceremony inventory,
and emits only a signed redacted receipt. The inner runner copies all 18 production
unit/target files and all 12 production ELF byte-for-byte. It contains no
production drop-in, `ExecStart` replacement or in-place unit mutation. Two
proof-only setup executables are hash-pinned separately and are not counted as
production services or binaries.

## Controlled transaction proof

The disposable contour installs exact copies only inside the container. No
unit is enabled and no host unit, binary, credential or state path is used.
The proof-only trigger remains outside the 31 production invocation count.

Each clean run must prove:

1. Phases 1-5 complete with the exact production binaries.
2. Phase 6 validates root admission, helper identity, receipt, Generation-2
   authority and projected credentials.
3. Attempt 1 is exactly `POST /v1/sessions`.
4. The request ends as `NETWORK_CONNECT_FAILURE` or request-level `TIMEOUT`
   under `network=none` with no HTTP status or body.
5. `TIMEOUT` is accepted only when `timeout_stage == "request"`.
6. All effect, dispatch, POST/DELETE and real-order flags remain false.
7. The native runner proves the exact production success-to-expected-fail-closed
   path and clean reset only.

Failure propagation across five representative graph edges and stale-output
rejection are not reimplemented by this runner. They are inherited from the
accepted Implementation R0-R1A evidence at
`6672819e357a3c2a2c1e73e5408c393da01913a1`, hash-bound by the R1 machine
contract. This avoids claiming a native fault matrix that this proof does not run.

The aggregate production target is expected to fail closed at Phase 6. The
outer proof runner succeeds only after the exact typed terminal oracle accepts
the durable evidence.

## Evidence policy

Both run 1 and run 2 preserve the following reviewable public artifacts:

- raw redacted root terminal envelope;
- redacted helper journal excerpt;
- exact typed derived request-boundary proof;
- SHA-256 bindings from the derived proof to both raw inputs;
- the hash-bound accepted failure/replay lineage;
- reset/destruction/uninstall receipts.

The temporary host ceremony must exist only at the exact tmpfs path
`/run/stage8b-g2-ceremony-source`. It is removed on both success and failure,
and a public destruction receipt is mandatory. Secret bytes, secret paths, account identifiers, environment files and
unredacted journals are forbidden in source, logs and handoff packages.

## Reset and second clean run

After run 1 the runner stops and resets the graph, removes every exact
transaction output, nonce, receipt, authority, package and credential
projection, and proves an empty transaction namespace. Run-1 private
materialization is destroyed. Run 2 rematerializes the same accepted public
identities into a fresh tmpfs projection and must produce the same fail-closed
request-boundary result.

After run 2 every unit, binary, state root and credential projection is
removed from the container. The container itself is then destroyed. Cleanup
failure makes the proof fail.

## Closed boundary

- Generation 2 active: `false`;
- R2B authorization: `NOT_ISSUED`;
- production credentials installed: `false`;
- external FINAM endpoint reached: `false`;
- broker GET, POST and DELETE: `false`;
- broker dispatch, Redis live and runtime-live: `false`;
- real orders: `false`.

Acceptance of the native proof will authorize only a separate real read-only
R2B issuance/run package. It will not authorize order execution or 8B-XE by
itself.
