# Stage 8B-P R2A6 source-adapter integration

R2A6 is the narrow production-source integration slice required by the R2A5
independent review. It does not authorize R2B and does not change the accepted
effect executable.

## Exact boundary

The accepted effect executable remains:

- effect build identity: `ca330934df540de69b52d0463a665a1ab0ff89fa13eeb663b162763cc6bc83a0`;
- effect executable SHA-256: `677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06`.

R2A6 adds a separately built adapter executable. Its only reviewed call path
is:

```text
source-authenticated Stage 5G restart package
  -> file-backed Stage 6 journal
  -> linear Stage 7B recovery-ready owner
  -> exact Stage 6 durable command
  -> Stage 8A accepted config/current sources
  -> publish_stage8b_r2a6_operational_sources_from_owner
  -> ten adapter-owned operational-source records
```

The adapter publishes the already accepted effect config/policy identifiers;
its own config/root digests are never substituted for the frozen effect-build
identity. The controlled manifest is then rebound to the exact Stage 7B/6/8A
record values and its run identity is recomputed before any receipt is issued.

The composition returns publication evidence only. It exposes no current-source
authority, arm, execution capability, transport handle or credential reader.

## Ownership contract

- immutable parent: `/var/lib/moex-trading`, UID 0, no group/other write;
- source directory: `/var/lib/moex-trading/operational-authorities`;
- source-adapter UID/GID: `8095` (`m8a8095`);
- directory mode: `0755`;
- source file mode: `0644`, link count 1;
- R2A5 downstream producers require owner UID `8095` and reject root/manual
  replacement;
- each producer and issuer retains its separate pre-existing UID.

The adapter service sandbox allows `AF_UNIX` only and applies
`IPAddressDeny=any`. Its exact source has no credential, AuthService, broker
GET, order POST/DELETE, operator-arm or dispatch entry point.

## Controlled qualification

The Linux rehearsal executes both PLACE and CANCEL from source-authenticated
durable fixtures. It calls the exact adapter binary before any producer runs,
asserts an initially empty operational-source directory, verifies all ten
records are UID 8095/mode 0644/single-linked, and then executes the retained
producer/issuer/package chain. The launcher and helper are the unchanged,
previously accepted R2A5 Linux artifacts; rebuilding the R2A6 producer tools
cannot silently replace that helper identity.

The controlled feature is excluded from default gateway/effect builds. It is
part of the separately qualified adapter artifact only.

## Closed surfaces

- production authorization: `NOT_ISSUED`;
- real FINAM credentials/Auth/GET: forbidden;
- operator arm and dispatch attempt: forbidden;
- accepted effect executable replacement: forbidden;
- order POST/DELETE and Stage 8B-XE: forbidden;
- Redis execution/runtime-live: forbidden.

Typed R2B operator decision remains mandatory in the immediately following
R2B package before any credential read.
