# Stage 8B-P R2B Implementation Package R0-R1

## Scope

This correction closes only the credential-custody, write-scope, Linux artifact,
and controlled transaction-proof findings against accepted implementation R0.
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
incremental compilation disabled. Both binaries must be byte-identical across
builds and must be stripped static x86-64 ELF files.
Exact values are frozen in the R0-R1 authority and build evidence.

## Dynamic rehearsal

The rehearsal runs natively inside a privileged disposable Linux/arm64 systemd
container after its external network is detached; it does not use QEMU. It uses
only canary credentials and performs real reads/writes from processes running
under the unit sandboxes. It also executes native controlled builder and signer
binaries against a controlled package and exercises the complete static graph
with injected failures and stale-output replay. Production deployability is
proved separately by the reproducible x86-64 ELF build evidence above.

Required results:

- builder cannot observe any credential canary;
- signer sees only its projected package key;
- supervisor cannot see package/helper/source signing keys but sees the exact
  broker-read subset;
- builder and signer cannot write their input roots;
- controlled draft construction and signing succeed;
- phase, producer, issuer, builder, and signer failures block downstream;
- old unsigned/signed output blocks a second transaction;
- no FINAM endpoint, real credential, production installation, or issuance is
  involved.

## Closed surfaces

`R2B = NOT_ISSUED`. Services remain uninstalled, disabled, and unstarted in
production. FINAM calls, POST/DELETE, broker dispatch, Redis live consumption,
runtime-live, and real orders remain closed.
