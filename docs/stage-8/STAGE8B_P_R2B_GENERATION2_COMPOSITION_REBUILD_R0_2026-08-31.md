# Stage 8B-P R2B Generation-2 Composition Rebuild R0

## Purpose

This package is the first composition step after independently accepted
Generation-2 custody closure at
`3029bab714f8b75daaba3946ed858426515b4165`. It selects the reviewed public
Generation-2 trust and account manifests in the production source boundary,
rebuilds every affected Linux executable, reissues the public helper
acceptance with the retained Generation-2 helper key, and repeats the accepted
Phase-6 compatibility rehearsal against account-key generation 2.

Selection in this package is a source and artifact composition decision. It is
not activation: no Generation-2 private file is installed on a production
host, no production run package is issued, and no FINAM or broker endpoint is
reachable.

## Accepted predecessor and custody

- accepted public backup/restore closure: `3029bab714f8b75daaba3946ed858426515b4165`;
- accepted archive SHA-256:
  `ee7deefa31dcf6b126408452f4772081ba20999c90ef58cf52df7b873869759f`;
- backup status: `VERIFIED`;
- restore verification: 13 signing seeds plus one account-binding key;
- disposable restore: deleted;
- retained private ceremony: outside Git, reports and handoff;
- Generation 2 active before and after this package: `false`;
- production R2B authorization before and after this package: `NOT_ISSUED`.

The accepted backup authority and both signed receipts remain byte-immutable.
This package consumes only their public fingerprints.

## Public composition

`stage8b-p-r2b-generation2-production-authority.json` binds exactly:

- the Generation-2 production trust-manifest SHA-256;
- the Generation-2 public-key-set SHA-256;
- the Generation-2 package-authorization public-key SHA-256;
- the Generation-2 account-key-manifest SHA-256;
- the unchanged accepted source-adapter authority SHA-256;
- `authorization_status = NOT_ISSUED`.

The same fail-closed validator is called by the source issuer, draft builder,
package issuer and helper-acceptance issuer. A Generation-1 manifest, mixed
generation, unknown manifest field, changed byte, different account-key path
or different source-adapter authority is rejected before signing or package
construction.

## Helper acceptance

The Linux helper is built first with a pinned container image and deterministic
release settings. Its exact SHA-256 is signed offline by the retained
Generation-2 helper-acceptance key in the existing non-authorization signature
domain. The operation:

- verifies the complete retained ceremony before reading the helper seed;
- checks private-to-public correspondence;
- emits only the public signed authority;
- does not read the package-authorization seed;
- does not install a credential or issue a run package;
- never records or prints the ceremony path.

The launcher embeds the new exact helper SHA-256. The accepted effect build
identity is unchanged because this stage does not rebuild or alter the bounded
effect executable.

## Reproducible Linux rebuild

Two fresh target directories build the complete affected set for
`x86_64-unknown-linux-musl` under the immutable builder image. Production
executables are classified separately from the offline public-authority tool.
Every pair must be byte-identical and must identify as stripped static PIE
x86-64 ELF. The launcher must contain the exact new helper digest.

The build uses a read-only source mount, `Cargo.lock`, no default features,
path remapping and `SOURCE_DATE_EPOCH=0`. The evidence is bound to the exact
source commit and Git tree.

## Generation-2 Phase-6 rehearsal

The accepted R0-R1A Phase-6 script is materialized only after its complete
base SHA-256 is verified. Exact-cardinality replacements select:

- Generation-2 trust and account manifests;
- Generation-2 account-key file;
- Generation-2 helper acceptance and helper digest;
- the rebuilt production source-authority issuer;
- the rebuilt package issuer, draft builder, helper and launcher.

The materialized script rejects all Generation-1 residue. It runs in a fresh
privileged Ubuntu systemd container with `--network none`; the retained
ceremony is mounted read-only only for this isolated rehearsal. The transient
rehearsal package must prove account generation 2 and all public composition
hashes, then disappears with the container. This does not create a production
authorization.

## Deliberately closed state

After successful completion:

- Generation-2 public source composition selected: `true`;
- affected production binaries rebuilt reproducibly: `true`;
- helper acceptance reissued under Generation 2: `true`;
- isolated Phase-6 rebound: `true`;
- Generation 2 active on a production host: `false`;
- production credentials installed: `false`;
- controlled installation executed: `false`;
- R2B production authorization: `NOT_ISSUED`;
- FINAM/AuthService/broker GET/POST/DELETE: closed;
- broker dispatch, Redis live, runtime-live and real orders: closed.

Independent acceptance of this package is required before any later controlled
installation or authorization artifact. Such work must be a separate stage.

