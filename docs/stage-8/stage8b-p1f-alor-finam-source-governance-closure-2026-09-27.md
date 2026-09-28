# Stage 8B-P1-f ALOR → FINAM source governance closure

Status: **SOURCE ACCEPTED / CURRENT-TREE AUTHORITY REBIND CANDIDATE — NO ACTIVATION**.

The source correction at
`aacd81c3a9181f9d0aa55d891f76cb573b453b8d`, parent
`be204471ddea51c940bdec3ae1e7b6bda8f06b03`, tree
`68efe82a75c242d2d5c82f304dbe8b5d74fb0330` is independently accepted.
The reviewed immutable archive SHA-256 is
`8dc7fa6d4091eb1ef75a76d7d7178fa3295687204b2110f2459bff644a3f0f5c`.
The acceptance document is
`FINAM_aacd81c_SOURCE_ACCEPT_REVIEW_2026-09-27.md`, SHA-256
`7b0dcf7131660881f711a2ec97bfdccee399f75c9ab50e8e009b52ba73c30a75`.

PAR02, O2A01 and O2A02 are closed at source level. The accepted profile is
`imoexf-baseline07-bo-only-paper-v1`, canonical profile SHA-256
`8f346b730760c8a70c4ab8576da60147a80a7c0668ba2068783ea2e5a2637872`,
with runtime configuration fingerprint
`6ac8994e5fc8777035c48c0b871b2d15a6662cdae6be88220f2bcdcadf0a244d`.

The accepted evidence is deliberately bounded. The 38/38 result compares the
FINAM Rust runtime with the frozen Python baseline07 reference over 7,307 M10
rows under diagnostic current-close fills; it is not full ALOR Rust executable
or broker lifecycle parity. The first-boot regression proves model-time
continuation through export/restore with synthetic binding hashes, not the full
Redis/V4/OS process path. The retained multi-UID harness proves Linux custody,
not execution of the final ELF under the exact production unit.

`Stage8bP1fO2RunnerErrorV1::StopNotProven.exit_code()` is 72. The current
operator binary maps runner and cleanup errors to process exit 70. The new O2
artifact review must assert process exit 70 unless a separately reviewed source
change introduces typed process-exit propagation.

The artifact at `1090de4` is superseded and must not create an operational
root. This closure authorizes only rebuilding exact O2 identities, source
schema/template, Linux binary and a non-activating package from `aacd81c`.
Binary/unit custody, actual stopped-proof behavior and OS process exit remain
artifact-review obligations.

No VPS activation, operational Redis mutation, FINAM POST/DELETE, broker
dispatch, runtime-live or real-order authority is opened. Updating the
current-tree manifest records the already accepted source; it grants no new
runtime capability. Synchronization with `main` still requires independent
acceptance of this governance-only successor and the protected-branch PR gates.
