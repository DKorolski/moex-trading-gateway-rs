# Stage 8B-P R2B Generation 2 Encrypted Offline Backup and Restore R0

## Purpose

This is the custody step immediately after the accepted Trust Rebind R0-R1.
It proves that the retained Generation-2 ceremony can be recovered from an
encrypted removable-media copy without placing a private value, private path,
or backup ciphertext in Git or in a review handoff.

This step does not select or activate Generation 2. It does not rebuild a
production binary, reissue helper acceptance, issue an R2B package, install a
credential, create a container, or reach FINAM.

## Custody layout

- the primary ceremony remains in persistent operator-owned storage;
- an `age` X25519 recovery identity is held in a non-cloud-synchronised,
  operator-owned directory on the internal encrypted volume;
- the encrypted archive is held on a separate removable volume;
- the orchestrator proves that the identity and ciphertext have different
  device identifiers;
- the identity, ciphertext and both absolute paths are excluded from Git,
  handoff evidence and command output;
- the removable filesystem may be FAT32 because only authenticated ciphertext
  and an optional public checksum are stored there.

The recovery identity is not a replacement for the Generation-2 ceremony. It
decrypts only the external backup and has no R2B authorization role.

## Operation

1. Require a clean committed source tree and build the two Rust attestors in a
   new dedicated `CARGO_TARGET_DIR` with `--locked --release`.
2. Verify exact primary inventory, owner, modes, links, ACL, file flags and
   extended-attribute policy.
3. Produce a canonical POSIX PAX stream directly into `age`; no plaintext tar
   file is created.
4. Hash and fsync the encrypted external artifact.
5. Decrypt it as a stream and extract through a fail-closed member validator
   into a new `0700` disposable directory on the FileVault volume.
6. Run the exact Generation-2 verifier over primary and restored copies and
   require identical public manifests and all 13 signing plus one account-key
   bindings.
7. Sign a public-only restore receipt in the dedicated domain
   `stage8b-p-r2b-generation2-backup-restore-receipt-v1`.
8. Remove the complete disposable restore root, prove absence, and sign a
   destruction receipt in the separate domain
   `stage8b-p-r2b-generation2-restore-destruction-receipt-v1`.

The destruction receipt asserts logical deletion on a FileVault-protected
volume. It deliberately does not claim physical secure overwrite.

## Public evidence

The source-controlled result contains only:

- ciphertext filename, byte length and SHA-256;
- recovery recipient SHA-256, not the private age identity;
- public Generation-2 fingerprints;
- exact 13 + 1 verification counts;
- verifier and destruction-attestor binary SHA-256 values;
- verifier source, Cargo.lock, age/age-keygen binary hashes and tool versions;
- signed restore and destruction receipts;
- fail-closed status and closed-surface declarations.

The encrypted archive itself remains outside the repository. The handoff
safety checker must reject the archive, age identity, ceremony filenames,
private key bytes, local custody paths and unexpected secret-like members.

## Metadata policy

Extended ACL and non-empty BSD file flags are rejected. The macOS
`com.apple.provenance` xattr is tolerated on the primary copy because it does
not extend access; all other xattrs are rejected. Archive construction omits
xattrs, so the restored copy must also have no unexpected xattr.

## Deliberately closed state

After successful backup and restore attestation:

- encrypted backup status: `VERIFIED`;
- Generation 2 active: `false`;
- Generation 2 selected by production composition: `false`;
- helper acceptance reissued: `false`;
- affected production binaries rebuilt: `false`;
- Phase-6 rebound: `false`;
- production credentials installed: `false`;
- controlled installation: `false`;
- R2B package authorization: `NOT_ISSUED`;
- FINAM/AuthService/broker GET/POST/DELETE: closed;
- broker dispatch, Redis live, runtime-live and real orders: closed.

## Next stage after independent acceptance

Only after this artifact is independently accepted may a separate composition
package select the reviewed Generation-2 public authority, rebuild and hash-pin
affected Linux binaries, reissue helper acceptance, and repeat the isolated
Phase-6 rehearsal. Controlled Installation remains blocked until that later
package is independently accepted.

