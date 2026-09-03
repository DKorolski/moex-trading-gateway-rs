# Stage 8B-P1-c R1 Redis restart and group-continuity closure

Status: source implementation review candidate.

Reviewed predecessor:
`a85ef845f86f99bcfd45654792cc688240457d3d`.

Accepted P1-b predecessor:
`ed6d98cb2bbc70c36e1033c6215d64dd6218cedf`.

This narrow correction closes the two P1 findings from the independent P1-c
review. It changes only real-Redis source acquisition and consumer-group
continuity. P1-b semantics, S1 command binding and the accepted command Lua
publication transaction are unchanged.

## Claim before fresh input

An ordinary durable `Ready` owner now acquires its next source as follows:

```text
XPENDING <M10 stream> <group> - + 2

0 pending
  -> XREADGROUP ... > COUNT 1

1 pending
  -> bounded XAUTOCLAIM using the fresh boot consumer and retained cursor
  -> process that exact entry before any fresh M10

more than 1 pending
  -> AmbiguousReadyPendingEntries
  -> no XREADGROUP > and no Hybrid callback
```

If the sole pending entry has not reached `claim_idle_ms`, acquisition returns
`PendingNotClaimable` together with the same linear composition owner. The
caller may retry later without rebuilding the durable owner. It cannot read a
newer source while the old PEL entry exists.

## Initialization versus attachment

Redis namespace creation and normal attachment are separate public operations:

- `initialize_stage8b_p1_redis_namespace` is the one-shot initialization path;
- `attach_stage8b_p1_redis` is verify-only and is used for normal/restart
  attachment.

Initialization may create the two streams and groups only when both stream
keys are absent. A repeated initializer is idempotent only when both streams
remain empty and each has exactly its expected group at frontier `0-0`, with
zero pending entries and zero consumers. Historical data, partial setup,
wrong types, missing groups or extra groups fail as `NamespaceNotFresh`.

Attachment executes no `XGROUP CREATE`. It requires both stream keys, exactly
the expected M10 and Stage 7 command groups, and parseable delivery frontiers.
Missing or inconsistent state returns `GroupMissing` and performs no repair.

## Already-acknowledged continuity

The zero-intent ACK-only restart path refreshes verify-only group evidence
before classification. Absence from the exact PEL is accepted as
`AlreadyAcknowledged` only when the current M10 group `last-delivered-id` is at
or beyond the durable source ID. A group externally recreated at `0-0` cannot
convert an undelivered retained entry into successful ACK evidence.

Operational ACL/noeviction policy remains a later deployment requirement; R1
does not claim that Redis can cryptographically identify an externally
recreated group.

## Real Redis evidence

The original eight P1-c tests remain green. R1 adds seven scenarios:

1. stale Ready-state A is reclaimed before fresh B;
2. a not-yet-claimable A returns retry ownership and does not deliver B;
3. two Ready-state PEL entries fail closed without a callback;
4. M10 group deletion after zero-intent ACK makes restart attach fail and does
   not recreate the group;
5. externally recreated M10 group at `0-0` cannot satisfy
   `AlreadyAcknowledged`;
6. Stage 7 command-group deletion makes restart attach fail without repair;
7. initializer rejects a historical M10 stream whose group is missing.

The in-memory mutation harness now has fourteen cases. In addition to the ten
P1-c guards, it rejects removal of Ready XPENDING-before-XREADGROUP, fresh read
from a stale-Pending branch, `XGROUP` repair in attach and unverified
already-acknowledged classification.

## Closed surfaces

- governance/current-tree authority rebinding until independent R1 acceptance;
- operational Redis DB 0;
- deployable P1 supervisor and Stage7 service attachment;
- paper provider and ACK/order/trade/position feedback;
- FINAM transport, POST/DELETE and broker dispatch;
- runtime-live, real orders and unattended execution;
- P1-d source implementation.
