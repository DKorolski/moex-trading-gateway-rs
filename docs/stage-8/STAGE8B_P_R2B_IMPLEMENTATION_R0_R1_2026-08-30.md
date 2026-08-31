# Stage 8B-P R2B Implementation Package R0-R1A

## Scope

This correction closes only the Phase-6 binary/path compatibility and immutable
handoff self-verification findings against implementation R0-R1. The accepted
credential-custody and dedicated write-root corrections remain unchanged.
The 31-service/six-phase design, read contract, package schema, receipt
semantics, and broker boundary are unchanged.

Predecessor: `da83f5922d9e2a9a5a1db3e581d2d9f55d810d81`.

## Credential boundaries

- The draft builder receives no credentials. The complete R2A5 credential tree
  is hidden with `InaccessiblePaths`, its capability and ambient sets are
  empty, and its network namespace is private.
- The package issuer receives only `package-authorization.ed25519` through a
  read-only systemd bind projection. Its source credential tree is hidden, capability and
  ambient sets are empty, and its network namespace is private.
- The root supervisor and its helper receive a second read-only projection
  containing only account ID, FINAM read secret, and account-binding keys.
  Package authorization, helper acceptance, source issuer private keys, and
  the source credential root are absent from that namespace.

The production signer accepts only the systemd projected credential path.
The bind projection is used instead of `LoadCredential` because nested systemd
mount namespaces on the qualification Docker host discard the latter's
service-private mount. The source tree remains inaccessible and the signer sees
exactly one read-only key file.
The compile-time controlled-custody feature retains its isolated fixture
fallback solely for old no-network qualification scripts; that feature is not
present in either production ELF.

## Write boundaries

- Builder output:
  `/var/lib/moex-trading/stage8b/r2a5/draft-output/r2b-run-package.unsigned.json`.
- Signer output:
  `/var/lib/moex-trading/stage8b/r2a5/signed-output/r2b-run-package.json`.

Both parent input roots are read-only in their respective unit namespaces.
The existing create-new, no-follow, no-replace, file-fsync, and directory-fsync
semantics remain unchanged.

## Linux artifact closure

Two clean `linux/amd64` release builds are made by a digest-pinned native ARM64
cross toolchain from the same read-only source mount with separate target
directories, no default or controlled-custody feature, path remapping, and
incremental compilation disabled. All four binaries must be byte-identical across
builds and must be stripped static x86-64 ELF files. The exact four-artifact
set is the draft builder, package signer, accepted helper, and launcher. Helper
acceptance is signed by an offline independent Ed25519 ceremony key, and the
launcher is rebuilt only after the accepted helper SHA is frozen. Exact values
are frozen in the R0-R1A authority and build evidence.

## Dynamic rehearsal

The earlier native Linux/arm64 rehearsal remains evidence for unit sandbox and
failure propagation. R0-R1A adds a separate Linux/amd64 QEMU/systemd proof with
no external network. It executes the exact packaged production builder, signer,
launcher, and accepted helper. Public production authorities are paired with
one-time offline ceremony material that is never committed or packaged. The
helper verifies identity, sealed receipt, production authority and only the
projected supervisor credentials before reaching the expected no-network
terminal; the root launcher persists terminal evidence.

Required results:

- builder cannot observe any credential canary;
- signer sees only its projected package key;
- supervisor cannot see package/helper/source signing keys but sees the exact
  broker-read subset;
- builder and signer cannot write their input roots;
- exact production draft construction and signing succeed;
- exact production launcher consumes the new signed-output path;
- exact production helper consumes only the new supervisor credential root;
- phase, producer, issuer, builder, and signer failures block downstream;
- old unsigned/signed output blocks a second transaction;
- no FINAM endpoint, real credential, production installation, or issuance is
  involved.

## Immutable handoff self-verification

The checker and negative harness accept an explicit `--artifact-root` and also
auto-detect exactly one of the working-tree or handoff ELF roots. Handoff
generation performs a fresh extraction and reruns the checker, all 35 targeted
negative mutations, packaged ELF hash verification, and archive safety without
copying artifacts back into a reports tree.

## Closed surfaces

`R2B = NOT_ISSUED`. Services remain uninstalled, disabled, and unstarted in
production. FINAM calls, POST/DELETE, broker dispatch, Redis live consumption,
runtime-live, and real orders remain closed.
