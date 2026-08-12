# Stage 7B-b — durable path and single-writer ownership

Status: Stage 7B-b-R2 review-closure candidate; independent acceptance pending.

Accepted predecessor:
`a947c24bb413a91c5eb0ad97f4ac0b402bfd0641` (Stage 7B-a-R1 CLOSED).

## Implemented scope

- a broker-neutral `runtime-durable-service` crate with no Redis, FINAM,
  transport, broker dispatch or live dependency;
- canonical durable directory name derived from the validated authenticated
  `Stage6dOperationalIdentityConfig` SHA-256;
- absolute, existing, canonical, non-symlink durable root and exact identity
  directory binding;
- a retained non-cloneable root-directory FD bound to the originally observed
  `dev/ino`; external root pathname drift is rejected;
- a retained trusted parent-directory FD plus identity-scoped namespace-lock
  lease serialize only this operational root name, so a replacement root
  cannot admit a second cooperative writer while unrelated identities remain
  independent;
- all authoritative child opens resolve with `openat` relative to that same
  root FD, so a renamed/replaced external pathname cannot redirect journal or
  lock acquisition;
- authoritative regular files must have one filesystem link, preventing a
  journal/lock/seal inode from aliasing another durable identity;
- no-follow validation for journal, writer-lock and future recovery-seal paths,
  plus a non-symlink `tmp/` boundary;
- `O_NOFOLLOW | O_CLOEXEC` for writer-lock and Stage 6 journal opens;
- first-boot journal creation requires a borrowed linear Stage 6 authorization
  whose deployment ID matches the authenticated operational identity;
- non-blocking kernel `flock(LOCK_EX)` on the identity-scoped parent namespace
  lock, anchored root-directory FD and sidecar lock FD, acquired before the journal is opened and
  retained inside the same non-cloneable authority;
- root and named-lock inode identity are rechecked across every open phase and
  before every later journal operation;
- all public writable constructors recompute the complete operational identity
  digest and require exact equality with the root-bound digest before first-boot
  authorization, kernel lock acquisition or any journal mutation;
- real subprocess proof that a second process is rejected while the first
  holds the authority and that abrupt child death releases the kernel lock;
- real phase-barrier proof that root replacement after lock acquisition fails
  before journal open without adopting the replacement tree;
- real lock-path replacement proof that a new sidecar inode cannot admit a
  second writer while the first root-directory lease remains live;
- an ordered storage capability boundary ending at `StorageReady`; Redis
  consumer attach remains impossible because this crate has no Redis
  dependency or API.

The sidecar lock file may remain after shutdown. Its existence is never lock
authority. The primary namespace guard is the kernel lease on the anchored
root-directory FD; the sidecar descriptor is an additional identity-checked
lease and diagnostic artifact.

## Filesystem threat model

This slice supports local Unix filesystems with process-shared `flock`, stable
directory descriptors, `openat`, link counts and durable file/directory sync.
Network/distributed filesystems are not an accepted deployment surface. The
durable parent is an operator-provisioned trusted local namespace. Mutation of
the identity root or lock pathname by another local process is nevertheless
detected and fails closed at every writable boundary covered by this slice.

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
