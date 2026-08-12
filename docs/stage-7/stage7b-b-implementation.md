# Stage 7B-b — durable path and single-writer ownership

Status: implementation candidate; independent acceptance pending.

Accepted predecessor:
`a947c24bb413a91c5eb0ad97f4ac0b402bfd0641` (Stage 7B-a-R1 CLOSED).

## Implemented scope

- a broker-neutral `runtime-durable-service` crate with no Redis, FINAM,
  transport, broker dispatch or live dependency;
- canonical durable directory name derived from the validated authenticated
  `Stage6dOperationalIdentityConfig` SHA-256;
- absolute, existing, canonical, non-symlink durable root and exact identity
  directory binding;
- authoritative regular files must have one filesystem link, preventing a
  journal/lock/seal inode from aliasing another durable identity;
- no-follow validation for journal, writer-lock and future recovery-seal paths,
  plus a non-symlink `tmp/` boundary;
- `O_NOFOLLOW | O_CLOEXEC` for writer-lock and Stage 6 journal opens;
- first-boot journal creation requires a borrowed linear Stage 6 authorization
  whose deployment ID matches the authenticated operational identity;
- non-blocking kernel `flock(LOCK_EX)` acquired before the writable journal is
  opened and retained inside the same non-cloneable authority;
- real subprocess proof that a second process is rejected while the first
  holds the authority and that abrupt child death releases the kernel lock;
- an ordered storage capability boundary ending at `StorageReady`; Redis
  consumer attach remains impossible because this crate has no Redis
  dependency or API.

The sidecar lock file may remain after shutdown. Its existence is never lock
authority; only the kernel lease on its descriptor is authoritative.

## Carry-forward aggregate witnesses

B-024 and B-025 establish the storage-side lifetime and ordering boundary in
this slice. When the actual Redis durable service is introduced, their proof
map rows must be supplemented with service-path witnesses showing that the
lease survives the entire consumer lifetime and consumer attach occurs only
after `StorageReady`.

## Intentionally closed

- authenticated recovery seal and atomic seal replacement;
- Stage 5G + Stage 6 cross-process restart composition;
- Redis consumer, ACK/DLQ/XACK settlement and readiness;
- broker/FINAM transport, POST/DELETE and runtime-live;
- X01-X20 aggregate crash matrix beyond the writer-lock subprocess cases.
