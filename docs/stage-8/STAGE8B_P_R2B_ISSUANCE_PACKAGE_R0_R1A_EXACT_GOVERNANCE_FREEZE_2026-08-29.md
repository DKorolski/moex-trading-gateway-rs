# Stage 8B-P R2B Issuance Package R0-R1A — exact governance freeze

## Scope

R0-R1A is a governance-only microfix over the substantively accepted R0-R1
candidate. It freezes the exact future issuance transaction before any service,
target, binary, credential, FINAM call or broker effect is implemented.

The R2B authorization remains `NOT_ISSUED`. The future activation target,
operator inputs and signed run package remain absent.

## Exact draft-builder inputs

The machine authority freezes seven ordered inputs. Each entry includes its
exact path, owner UID/GID, mode, producer, run-nonce binding and freshness rule:

1. run nonce;
2. run manifest;
3. trust manifest;
4. account-key manifest;
5. operator decision;
6. accepted helper authority;
7. the exact eleven authority receipts.

The future draft builder is root-owned, networkless and has no signing-key or
credential-path access. Its only output is the atomic, durable, mode `0600`
unsigned `R2a5RunPackage` draft.

## Exact authority receipt inventory

The authority freezes eleven ordered source records. Every record binds the
source name to its producer service and UID, issuer service and UID, receipt
path, authority type, evidence schema, source-specific maximum age and current
run nonce requirement. The exact records are defined once in
`stage8b-p-r2b-issuance-package-r0-r1-authority.json` and independently pinned
by the fail-closed checker.

## Exact transaction and signer

The six phase objects are frozen as complete ordered values: ordinal, name,
target, predecessor target and ordered service list. There are exactly 31
service invocations. Phase 5 runs the draft builder and then the sole package
signer. The signer identity, fixed unsigned input, root-only mode `0600`
credential, signed output custody and durable atomic publication are exact.

No skipped, failed, missing, reordered or additional phase component may allow
the successor phase to proceed.

## Exact read-contract refresh binding

The checker freezes the refresh-evidence path, six-document inventory, snapshot
path and SHA-256, and the 1,800-second activation maximum age. This package does
not perform activation refresh or use credentials.

## Enforcement

The original R0-R1 harness retains 25 negative cases. The R0-R1A harness adds
29 exact-governance mutations covering builder inputs, source identities and
paths, phase topology, builder/signer identity and read-refresh binding. The
combined required negative count is 54.

No Rust, Cargo, deployment unit or production architecture is changed by this
microfix. FINAM transport, HTTP POST/DELETE, Redis command consumption, broker
dispatch, runtime-live, strategy-live and real orders remain closed.
