# Stage 7B-d Design R1 — Implementation Contract Addendum
## Durable settlement authority, ACK/DLQ provenance and Redis atomic-settlement semantics

**Project:** broker-neutral MOEX runtime / FINAM migration  
**Accepted Stage 7B-c predecessor:** `c57ae8d5f98bbb11df0a81f78262d3916b276d81`  
**Reviewed but not accepted design candidate:** `09a22765ae6ee37b304bfed6492bd103da44360d`  
**Target:** Stage 7B-d design freeze only  
**Date:** 2026-08-13  
**Normative artifacts:** this document + `STAGE7B_D_DESIGN_R1_ACCEPTANCE_MATRIX_2026-08-13.csv`

---

# 1. Purpose

This is a narrow normative addendum to the existing Stage 7B-d design. It freezes the authority semantics that must be unambiguous before Stage 7B-d production code is written.

Accepted decomposition:

```text
7B-d-a — lifecycle / seal-before-settlement
7B-d-b — atomic Redis ACK/DLQ + XACK
7B-d-c — composite readiness / supervision / PEL recovery
```

No implementation slice may begin until this Design R1 is independently accepted.

# 2. Inherited boundaries

```text
Stage 6 remains the sole lifecycle/execution authority.
Stage 7B-c owner retains Stage6 runtime + writer/root lease.
Recovery seal is restart/authentication binding, not lifecycle DB.
Process-memory ACK maps are not restart authority.
Redis identities remain transport-only.
No exactly-once external broker-effect claim.
FINAM POST/DELETE, broker dispatch, runtime-live and real orders remain closed.
```

# 3. Terminal ACK authorization

A terminal ACK may be authorized only after:

```text
durable Stage6 terminal outcome
-> durable RequestFinalized
-> current Stage6 checkpoint/frontier
-> next authenticated Stage7B recovery seal committed
-> committed seal reread + canonical/HMAC/checkpoint validation
```

Only then may the owner mint a capability conceptually named `DurableAckAuthorized`.

Required properties:

```text
non-Clone
non-Copy
non-Serialize
non-Deserialize
non-publicly-constructible
single-consumption / linear
```

It binds at least:

```text
operational identity SHA-256
StrategyRequestId
canonical command digest
durable request/client identity
Stage6 final disposition / canonical ACK classification
Stage6 checkpoint/frontier fingerprint
committed seal generation
committed seal commitment/fingerprint
canonical ACK fingerprint
settlement kind = ACK
```

A capability for request A cannot settle request B.

# 4. Redis ACK settlement plan

Combine durable authorization with exact Redis source context to form `RedisAckSettlementPlan` or equivalent.

It binds:

```text
validated paper namespace
command stream
consumer group
Redis entry ID
settlement kind
canonical ACK fingerprint
durable authorization fingerprint
```

Redis transport fields never enter Stage6 identity.

# 5. ACK and poison-DLQ provenance are separate

Terminal ACK:

```text
BrokerCommand
-> Stage6 lifecycle
-> durable finalization
-> committed seal
-> DurableAckAuthorized
-> atomic ACK + XACK
```

Permanent pre-Stage6 poison:

```text
malformed/schema-invalid Redis entry
-> prove no Stage6 admission
-> prove no Stage6 journal/request mutation
-> PoisonDlqAuthorized
-> atomic redacted DLQ + XACK
```

`PoisonDlqAuthorized` is independently linear/non-serializable and binds source entry, poison reason and redacted payload fingerprint.

These states never become poison:

```text
IdentityConflict
ConflictingDuplicate
ReconciliationRequired
RecoveryBlocked
DurabilityUncertain
provider outcome unknown
post-Stage6 authority hold
```

They remain pending and never ACK/DLQ/XACK.

A pre-Stage6 poison does not advance the Stage6 recovery seal because Stage6 state did not change.

# 6. Stable Redis settlement identity

Stable per-entry settlement key derives only from:

```text
paper namespace / cluster hash tag
source command stream
consumer group
Redis entry ID
settlement kind
```

It MUST NOT depend on ACK/DLQ payload fingerprint.

Marker value stores:

```text
schema/version
settlement kind
exact ACK/DLQ fingerprint
published output stream
published Redis output ID
canonical/duplicate classification where applicable
```

Exact retry:

```text
same key + same fingerprint
-> return committed result
-> no second XADD
```

Conflict retry:

```text
same key + different fingerprint
-> fail before XADD/XACK
```

# 7. Request-level canonical ACK marker

Maintain a stable request-level publication marker that records:

```text
StrategyRequestId / stable request lookup identity
canonical terminal ACK fingerprint
canonical output Redis ID
publication-known state
```

Semantics:

```text
first terminal publication
-> canonical ACK + marker

same-entry response-loss retry
-> return existing committed settlement
-> no second XADD

new exact duplicate entry later
-> Duplicate/DuplicateCommand ACK
-> canonical marker unchanged

conflicting duplicate
-> no ACK/DLQ/XACK
```

# 8. Redis atomic-script rules

One atomic Redis primitive handles ACK or DLQ settlement plus XACK.

All keys used in one script share one intentional Redis Cluster hash slot.

For a new settlement marker:

```text
before first write
-> validate key types/schema/hash-slot/arguments
-> validate marker/request-marker conflicts
-> validate payload fingerprint
-> prove source entry is pending in expected group
```

If any precondition fails: no XADD, no marker, no XACK.

For an already committed exact marker retry, source PEL membership is not required and no second XADD occurs.

All expected semantic/type/conflict checks must happen before first Redis mutation. Lua atomicity is not treated as rollback for arbitrary script errors.

# 9. Seal commit uncertainty

If the caller cannot determine whether seal rename/fsync committed:

```text
no settlement capability
readiness false
no ACK/DLQ/XACK
```

Recovery is by rereading/reconciling the on-disk committed seal or reconstructing/restarting the owner.

Blind cached `generation + 1` retry is forbidden.

# 10. Redis durability scope

B-059 guarantees idempotent recovery for:

```text
Redis script committed
but process/client lost the response or died
```

It does not claim Redis persistent storage cannot roll back under server/storage/failover failure.

Source stream, markers and ACK/DLQ streams must share one Redis durability/failover domain.

# 11. Slice ownership

## 7B-d-a

May close:

```text
B-043..B-051
B-054..B-056
```

B-052/B-053 remain pending even if d-a adds semantic helpers.

No Redis production dependency is required.

## 7B-d-b

Owns:

```text
B-057..B-063
```

Atomic Redis ACK/DLQ/XACK settlement only.

## 7B-d-c

Owns:

```text
B-064..B-070
```

and final real-Redis restart evidence for:

```text
B-052
B-053
```

# 12. Design R1 descriptor minimum

```json
{
  "stage": "7B-d-design-R1",
  "accepted_predecessor": "c57ae8d5f98bbb11df0a81f78262d3916b276d81",
  "production_diff_from_accepted_stage7b_c": false,

  "settlement_authorization_linear": true,
  "settlement_authorization_serializable": false,
  "settlement_authorization_exact_request_bound": true,
  "settlement_authorization_seal_generation_bound": true,
  "settlement_authorization_checkpoint_bound": true,
  "settlement_authorization_payload_fingerprint_bound": true,

  "separate_ack_and_poison_capabilities": true,
  "poison_requires_zero_stage6_mutation": true,
  "poison_no_stage6_seal_advance": true,
  "holds_never_dlq_or_xack": true,

  "stable_settlement_key_excludes_payload_fingerprint": true,
  "marker_value_contains_payload_fingerprint": true,
  "request_canonical_ack_marker": true,
  "new_settlement_requires_expected_pel": true,
  "marker_retry_does_not_require_pel": true,
  "lua_validates_before_first_write": true,
  "single_hash_slot_required": true,

  "ambiguous_seal_commit_requires_reread": true,
  "redis_response_loss_scope_explicit": true,

  "d_a_rows_exclude_b052_b053": true,
  "d_c_closes_b052_b053": true,

  "cross_process_exactly_once_claimed": false,
  "finam_post_delete": false,
  "runtime_live": false,
  "real_orders": false
}
```

# 13. Required negative mutations

The design-negative harness must detect at least:

- ACK capability made Clone/serializable/reconstructible;
- request/command binding removed;
- seal generation/checkpoint binding removed;
- ACK fingerprint binding removed;
- ACK/DLQ authorization merged;
- DLQ allowed after Stage6 admission/hold;
- zero-Stage6 poison proof removed;
- fake finalization/seal required for poison;
- payload fingerprint inserted into stable marker key;
- stored fingerprint conflict check removed;
- request canonical marker removed;
- new-settlement PEL precondition removed;
- committed retry incorrectly requires PEL;
- validation moved after first Redis write;
- cluster hash-slot rule removed;
- blind seal generation retry allowed after ambiguous commit;
- Redis rollback durability overclaimed;
- B-052/B-053 marked implemented in d-a;
- d-c real-Redis closure responsibility removed;
- production Redis/FINAM/live implementation opened in design R1.

Exact negative count is descriptor-pinned.

# 14. Exit criteria

Design R1 is accepted only if:

1. ACK authority is exact-bound and linear.
2. Poison DLQ has separate provenance.
3. Hold/uncertain states can never settle.
4. Stable settlement key is independent of payload fingerprint.
5. Marker value detects payload conflict.
6. Canonical ACK publication semantics are frozen.
7. New settlement requires expected PEL membership before first write.
8. Exact committed retry does not re-XADD.
9. All Lua semantic validation precedes mutation.
10. Seal ambiguity cannot mint settlement authority.
11. Redis response-loss scope is explicit.
12. B-052/B-053 remain pending until real-Redis restart evidence.
13. d-a/d-b/d-c row ownership is machine-readable.
14. Production code remains unchanged from accepted Stage 7B-c.
15. FINAM/live surfaces remain closed.

# 15. Programmer instruction

Do not implement production code yet.

First update only:

```text
stage7b-d-design.md
entry descriptor
design checker
design negative harness
row-to-slice governance
```

After independent ACCEPT of Design R1:

```text
open Stage 7B-d-a only
```

Do not begin d-b or d-c opportunistically.
Do not open FINAM/runtime-live/real orders.
