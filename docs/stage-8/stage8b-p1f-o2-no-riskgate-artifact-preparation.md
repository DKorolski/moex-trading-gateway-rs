# No-riskgate O2 binary artifact — review candidate

2026-10-01. Status: BINARY_ARTIFACT_REVIEW_CANDIDATE_NOT_INSTALLABLE.
The local compiler blocker is resolved by the owner-authorized Docker Desktop
restart; this is not independent artifact acceptance or operational O2 success.

Accepted compiled baseline: PR #11 normal merge
`ca1e5da7ea41eec219bce1cfe2bdf4b8d63d9029`, tree
`0dd557b97b1cc208ffa8ae6228cef03c1bbabc19`. The tree equals accepted
`8e7a647`; both required CI jobs passed before merge. No production Rust,
Cargo, CI workflow or installed-file change is proposed here. Only status/roadmap
control inventory hashes are refreshed; production authority is unchanged.

## Completed preparation

The new build wrapper pins this merge while retaining the historical build
scripts and their source/image pins. Artifact tools package three release
ELFs with raw commits and two reconstructed Git trees (build versus packaging),
source manifests and logs. Clean release build from the accepted ref completed
in 3m07s with Cargo locked/offline and Docker network none. The inherited image
and Cargo feature selection were unchanged. Three pre-existing default-feature
publisher dead-code warnings remain in the build log; no warning suppression or
production correction was made. Rust is 1.98.0 inside this inherited build image;
the accepted source CI used Rust 1.95.0, not this artifact build.

The executable probe linked to the exact release rlib and obtained both profile
fingerprints. The real ELF smoke passed legacy admission, no-riskgate V2
admission, rejection of mixed profile hashes/fingerprints, custody negatives
and empty durable/control roots. All three qualification commands exited 0.
The profile V2 fingerprint is
`665f18112142f6c9aee1b613e341870f0d559d783fe41df725e26415507e6290`.
The shared semantic profile hash remains
`6d7dff3543993b7727a161f85af3037f74762ecd2ce8332e0594b9c92d987c38`.

| Built ELF | SHA-256 |
|---|---|
| materializer | `e9805b7437d61f33bc2144d5c609809ea997094be3b02df21bda8f8667983c73` |
| operator | `12a679dd44b25cbc571937d16c6333ee20b97a81b6fd706436e0ad25394e7e4c` |
| supervisor | `47ec8ab20345f29f1915f297e541f29a554131aa10ae1aadb9559069d769f074` |

Execution proof is local Docker Linux/amd64 on an ARM Mac, not a native VPS
bootstrap. No Redis or FINAM calls were made. The materializer proof here is
safe custody rejection; successful live collection is still the future bounded
O2 observation, not inferred from source tests or this artifact's existence.

The ZIP is a binary review artifact, not an operational installation
package. Frozen source/policy examples and a supervisor smoke fixture are not
fresh operational inputs. Real calendar/cutoff, operational identity and updated
installation manifest must be bound before installation acceptance. Retain
Failed generation 1 / sequence 4 and preceding Expired/2 history; never reset
genesis or reuse the old sequence-2-only update as a sequence-4 authorization.

## Retained failed attempt and recovery

The inherited image
`rust@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922`
failed at `rustc --version --verbose` during the first local Docker amd64 attempt, before
Cargo/project compilation. The retained build log is
`/tmp/stage8b-o2-nrg-ca1e5da-build/build.log`; its first diagnostic is
`qemu: uncaught target signal 11 (Segmentation fault) - core dumped`.
The stuck container was explicitly killed, so the outer builder returned 137.
That is not represented as a completed compiler test.

Bounded/disposable probes also failed with the direct toolchain binary,
`QEMU_CPU=max`, and container-only unconfined seccomp. The official
`rust:1.95-bookworm` image was downloaded for a separate version-only probe:
digest `sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1`.
It also failed before compilation; it was not substituted into the build recipe.
An already cached Rust 1.90 version probe produced SIGSEGV in the compiler's
startup/allocation path. Direct user-mode QEMU 10.0.4 and 10.2.3 probes did not
resolve the failure. This localizes the blocker to compiler execution in this
environment but does not establish the underlying host/emulator root cause.

No global binfmt registration or Docker setting was changed by those probes.
An unprivileged binfmt status attempt could not mount binfmt_misc and
was not treated as a successful inspection. All containers created for this
attempt were stopped/removed; the existing ALOR buildkit was left running.
No VPS, Redis, FINAM endpoint or private credential was accessed. Subsequently,
the owner explicitly authorized a local Docker Desktop restart. After
`docker desktop restart --timeout 180`, the same pinned compiler preflight
passed; the existing ALOR buildkit automatically returned to running. The clean
build succeeded in a new directory, keeping the failed log unchanged. This
establishes recovery after restart, not a proven underlying emulator root cause.

## Reproduction and review handoff

The source snapshot at the ZIP root is the packaging commit. The compiled ref
is the separately pinned `ca1e5da` merge. Two manifests, raw commits and changed
preimages reconstruct both trees without Git. Never treat the packaging SHA as
a different Rust build or historical status documents as fresh operational proof.

From the extracted ZIP, without `.git`:

```sh
python3 scripts/stage8b_p1f_o2_no_riskgate_artifact.py check /absolute/path/to/archive.zip
python3 scripts/test_stage8b_p1f_o2_no_riskgate_artifact.py /absolute/path/to/archive.zip
python3 scripts/current_tree_authority_check.py
```

To reproduce build/qualification in a clean Git checkout with the pinned Docker
image and a public Cargo registry cache (no credentials):

```sh
python3 scripts/stage8b_p1f_o2_no_riskgate_build.py NEW_BUILD_DIR --registry-cache PUBLIC_REGISTRY_DIR
python3 scripts/stage8b_p1f_o2_no_riskgate_artifact.py qualify NEW_PROOF_DIR --build NEW_BUILD_DIR
```

The bounded postseal gate is
`scripts/stage8b_p1f_o2_no_riskgate_gate.py ARCHIVE NEW_GATE_DIR`. Its separate
`result.json` binds the exact ZIP hash, source inventory, command exit codes and
raw logs, including 12 archive mutations and 45 authority mutations. A declared
test inventory is not a PASS claim; the actual gate result accompanies the ZIP.
The generated directory `artifact-no-riskgate/` contains build/ELF evidence,
the accepted source review and the old failed compiler log. The archive safety
checker enforces regular/canonical members, exact inventories and raw Git
binding; it is not a general credential-content classifier.

Review this artifact/helper delta; do not reopen accepted source history absent
a concrete regression. Once accepted, prepare the terminal-history-preserving
installation package with fresh bound inputs. Installation and bounded O2 retain
their separate permission boundaries. No new source sub-stage or recovery
framework is introduced by this artifact work.
