# Stage 8B-P R2B Trust Rebind R0

## Outcome

The accepted generation-1 public trust set remains immutable historical
evidence, but its matching private ceremony is no longer available. It was
never used to issue an R2B package, install production credentials, or send a
FINAM request. Public manifests cannot be used to reconstruct private keys.

R0 therefore creates a distinct generation-2 candidate rather than silently
regenerating generation 1. This package does not activate the candidate and
does not resume Controlled Installation Execution R0.

## Scope

R0 adds:

- a generation-aware ceremony generator which preserves the accepted v1
  signature-domain key IDs while advancing every trust generation to `2`;
- a read-only verifier for exact custody modes, inventory, all private/public
  Ed25519 bindings, account-key binding, validity and manifest digests;
- public generation-2 trust and account-key projections;
- an additive loss/supersession record and fail-closed review gate.

Historical R2A5 and R2B accepted files and their checkers are not rewritten.
The unreviewed Execution R0 foundation remains isolated on its own branch.
`docs/current-status.md` is also left byte-immutable because it belongs to the
accepted GOV-CI control-plane manifest; this additive authority is the R0
status record until a separately governed status rebaseline is justified.
The current GOV-CI authority and its negative harness remain the governing
current-tree surface. The legacy Stage-5 text scanner is not modified or
reinterpreted in this package; its accepted replay remains owned by the
canonical current-tree CI gate.

## Custody boundary

The private generation-2 ceremony is held in persistent operator-owned storage
outside the repository and outside temporary directories. Its absolute path is
not source-controlled. Directories are `0700`; all 13 signing seeds and the
account key are `0600`; public manifests are `0644`. The verifier rejects
extra entries, symlinks, hard-linked files, ownership/mode drift, key mismatch,
generation drift and an ephemeral/source-tree path.

No private bytes, private path, `.env`, credential, or secret-derived value is
included in a handoff. Only public keys and SHA-256 values may leave custody.

## Backup gate

The primary persistent copy is not sufficient to activate production
composition. R0 deliberately records:

```text
encrypted_offline_backup_status = REQUIRED_NOT_VERIFIED
activation_without_verified_backup_allowed = false
```

An external encrypted backup and a redacted attestation must be completed by
the operator after R0 review. Until then, generation 2 cannot be selected,
installed, used to accept a helper, or used to issue a package.

## Generation semantics

`v1` in a key ID identifies the accepted signature/preimage domain. It is not
the mutable key generation. Rotation is represented by the typed `generation`
field and by the account `generation_id`. Renaming all key IDs to `v2` would
change the signature domain and require an unrelated runtime protocol
migration, so R0 keeps the IDs and advances every generation field to `2`.

## Activation sequence after review

1. Create and verify an encrypted offline backup; add only a redacted backup
   attestation to source control.
2. Rebind the selected public authority and all generated composition inputs to
   generation 2.
3. Reissue helper acceptance with the generation-2 helper key.
4. Rebuild and hash-pin every affected Linux production binary.
5. Update Phase-6 account materialization from generation 1 to generation 2.
6. Repeat the full isolated rehearsal and independent review.
7. Only a later, explicit authorization may resume Controlled Installation.

## Closed surfaces

R0 performs no container creation, production installation, FINAM network
access, AuthService call, broker GET, HTTP POST/DELETE, broker dispatch, Redis
live consumption, runtime-live transition, or real order. Authorization remains
`NOT_ISSUED`.
