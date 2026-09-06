# Stage 8B-P1-d4 crash/replay design R7 correction

Status: R7 design-only review candidate. P1-d4 source implementation remains
paused until independent R7 acceptance.

Design parent: `cb6e6ddf863f314cc96b5f8ac0a75809e8c6824a` (R6, HOLD).
Accepted design baseline: `e1ce6d3baec3974d8dfd05c2f3de00110e0605bf`
(R3, ACCEPTED). Business baseline:
`7dc7c802feca6e79d3a1a9902c181ad7b6afc506` (P1-d3, CLOSED / ACCEPTED).

R7 retains R6's accepted Stage6 V1 chain, sequence timing and post-`S_ack`
publication field set. It closes the remaining design gaps by precommitting an
exact Redis command-entry identity before XADD, exact-freezing package-aware
classifier precedence and defining canonical publication bytes plus checked
generation relations.

## Active proof inventory

```text
inherited base registry: 92 cells
corrected generated-Market v3 registry: 13 cells
active total: 105 exact positive cells
duplicate variant: required for every cell
conflict variant: required for every cell
```

The 13 generated-Market cells are derived from the same durable graph as R6.
Reservation selection creates no extra durable phase: a crash before the first
combined Prepublication persist belongs to the inherited S05 precommit cell;
GM00 starts only after the reservation-bearing package is persisted and reread.
Thus the total remains derivably `92 + 13 = 105`, not by preserving 105 as an
independent target.

## Strong precommitted publication identity

R7 chooses the preferred strong model. Before command XADD, the Redis wrapper
performs a read-only observation of the exact command stream
`last-generated-id`, computes its immediate successor and supplies both IDs to
the package builder. The first combined Prepublication package contains and
HMAC-covers:

```text
Stage8bP1d4CommandPublicationReservationV1
  schema_version = 1
  domain = moex.stage8b.p1d4.command-publication-reservation.v1
  source_stream
  source_group
  source_m10_redis_id
  semantic_batch_id_sha256
  strategy_request_id
  canonical_command_sha256
  canonical_envelope_sha256
  command_stream
  command_group
  command_stream_predecessor_id
  reserved_command_entry_id
  prepublication_package_generation
  publication_reservation_sha256
```

No XADD is reachable until that package is fsynced, reread, authenticated and
cross-validated. The reservation contains identity only: no Redis connection,
lease, raw capability, provider, XACK authority or Stage5C state.

### Exact Redis ID successor

Redis IDs are canonical unsigned decimal `milliseconds-sequence` pairs with no
leading zero unless the component is exactly `0`. The immediate successor is:

```text
if sequence < u64::MAX:
  (milliseconds, sequence + 1)
else if milliseconds < u64::MAX:
  (milliseconds + 1, 0)
else:
  fail closed before package persistence
```

The predecessor is the exact `last-generated-id` from `XINFO STREAM`, not the
last visible entry. This preserves deletion history. R7 permits no dynamic ID
repick inside the same lifecycle.

### Atomic publication protocol

The future Lua operation receives the authenticated reservation and uses
explicit `XADD command_stream reserved_command_entry_id`, never `XADD *`.
It validates the exact source/PEL/groups and follows only these branches:

```text
existing marker:
  every marker field and command_entry_id equals the reservation;
  exact reserved entry exists with the exact envelope bytes;
  return IdempotentExisting.

no marker:
  reserved entry must be absent;
  current last-generated-id must equal command_stream_predecessor_id;
  atomically XADD the exact reserved ID and SET the exact marker;
  returned ID must equal the reservation;
  return Published.

all other shapes:
  hard conflict before schedule/dispatch/provider/ACK/truth/XACK.
```

This closes the three publication crash cases:

- crash before XADD: the authenticated reservation republishes the same exact
  ID;
- XADD/marker response loss: exact marker and exact entry validate against the
  precommitted ID;
- original E1 removed and byte-identical E2 substituted: E2 cannot equal the
  reserved E1 and fails before effects.

If marker and E1 are deleted, Redis retains `last-generated-id = E1`; the
predecessor check fails closed. A full storage rollback that also restores the
predecessor may publish E1 again, preserving the same logical identity; such a
storage rollback is not interpreted as a distinct E2 publication.

The command stream is a single-writer P1 namespace during this lifecycle. Any
concurrent advancement between observation and Lua is a hard conflict; it does
not authorize choosing a later ID.

## Publication binding after XADD

The accepted-in-principle R6 binding is retained and extended by the exact
reservation identity:

```text
Stage8bP1d4CommandPublicationBindingV1
  schema_version = 1
  domain = moex.stage8b.p1d4.command-publication-binding.v1
  source_stream
  source_group
  source_m10_redis_id
  semantic_batch_id_sha256
  strategy_request_id
  canonical_command_sha256
  canonical_envelope_sha256
  command_stream
  command_group
  command_stream_predecessor_id
  command_entry_id = reserved_command_entry_id
  prepublication_package_generation
  publication_reservation_sha256
  prepublication_seal_generation
  prepublication_seal_commitment_sha256
  publication_binding_sha256
```

The reservation is mandatory in all combined phases. The full binding is
absent before XADD, reconstructed only from the matching reservation plus exact
atomic marker/entry, and persisted in combined `S_ack`. `AckCommitted` and
`TruthCommitted` require the identical HMAC-covered binding and revalidate it
before truth and XACK respectively. Publication disposition is an observation,
not identity, and is excluded from canonical binding bytes.

## Exact canonical encoding

Both reservation and binding use map-free fixed-order bytes. Primitive encoding
is exact:

```text
u16: two-byte unsigned big-endian
u32: four-byte unsigned big-endian
u64: eight-byte unsigned big-endian
lp_utf8(s): u32_be(byte_length) || strict UTF-8 bytes
UUID: RFC 4122 network-order 16 bytes parsed from canonical lowercase hyphen text
SHA-256 field: exactly 64 lowercase hexadecimal characters decoded to raw 32 bytes
Redis ID: canonical decimal parse then u64_be(milliseconds) || u64_be(sequence)
```

Noncanonical UUID text, uppercase/short digest, signed/overflow integer,
noncanonical Redis ID, NUL-containing token or invalid UTF-8 fails before
hashing.

Reservation body field order is exactly:

```text
u16 schema_version
lp_utf8(domain)
lp_utf8(source_stream)
lp_utf8(source_group)
redis_id(source_m10_redis_id)
raw_sha256(semantic_batch_id_sha256)
uuid_bytes(strategy_request_id)
raw_sha256(canonical_command_sha256)
raw_sha256(canonical_envelope_sha256)
lp_utf8(command_stream)
lp_utf8(command_group)
redis_id(command_stream_predecessor_id)
redis_id(reserved_command_entry_id)
u64 prepublication_package_generation
```

```text
reservation_canonical_bytes =
  b"moex.stage8b.p1d4.command-publication-reservation.canonical.v1\0"
  || u64_be(body_length) || body

publication_reservation_sha256 = SHA256(reservation_canonical_bytes)
```

Binding body field order is exactly:

```text
u16 schema_version
lp_utf8(domain)
lp_utf8(source_stream)
lp_utf8(source_group)
redis_id(source_m10_redis_id)
raw_sha256(semantic_batch_id_sha256)
uuid_bytes(strategy_request_id)
raw_sha256(canonical_command_sha256)
raw_sha256(canonical_envelope_sha256)
lp_utf8(command_stream)
lp_utf8(command_group)
redis_id(command_stream_predecessor_id)
redis_id(command_entry_id)
u64 prepublication_package_generation
raw_sha256(publication_reservation_sha256)
u64 prepublication_seal_generation
raw_sha256(prepublication_seal_commitment_sha256)
```

```text
binding_canonical_bytes =
  b"moex.stage8b.p1d4.command-publication-binding.canonical.v1\0"
  || u64_be(body_length) || body

publication_binding_sha256 = SHA256(binding_canonical_bytes)
```

The checked-in fixture
`fixtures/stage8b-p1d4-command-publication-binding-v1.json` freezes both exact
canonical byte strings and hashes. Fresh construction and every restart phase
must reproduce them byte for byte.

## Exact generation relations

Let the combined Prepublication package and covering seal have generations
`W0` and `G0`. The reservation binds `W0`; the full binding binds `W0`, `G0`
and the exact `G0` seal commitment. Checked addition is mandatory:

```text
combined S_ack:   write_generation W1 = W0 + 1
                  covering_seal_generation G1 = G0 + 1
combined S_truth: write_generation W2 = W1 + 1 = W0 + 2
                  covering_seal_generation G2 = G1 + 1 = G0 + 2
```

No equality between W and G is assumed. Overflow, skip, reuse, downgrade or a
binding to successor G1/G2 instead of original G0 is a hard conflict. Snapshot
revision and previous-revision checks remain inherited from the accepted
Stage5G package export.

## Package-aware classifier routing

R7 freezes one tri-state inspection of the already authenticated replacement
package:

```text
Absent:
  no P1-d4 generated-Market discriminator/projection exists.

PresentValid:
  exact composition, reservation, P1-d3 peer, source, semantic commit,
  request/command and package generation all cross-validate.

PresentInvalid:
  a P1-d4 discriminator/projection is declared but any decode, HMAC, peer,
  reservation or identity check fails.
```

Recovery routing is exact and ordered:

```text
1. Preserve accepted P1-d3 V3 routing.
2. Inspect the authenticated package for the P1-d4 tri-state.
3. PresentInvalid -> hard Blocked/Corrupt; no classifier fallback.
4. PresentValid -> run P1-d4 V1 routing before ordinary P1-d2:
     suffix 1: P1d4GeneratedMarketDispatchPending
     suffix 2: P1d4GeneratedMarketOrderPending
     suffix 3: reuse/wrap accepted P1-d2 structural candidate as
               P1d4GeneratedMarketPreFinalizationPending
     suffix 4: reuse/wrap accepted P1-d2 structural candidate as
               P1d4GeneratedMarketPreAckPending
   The ordinary P1-d2 return is unreachable in this branch.
5. Absent -> preserve the accepted ordinary order exactly:
     P1-d2 complete-suffix classifier -> generic P1 classifier.
```

The P1-d4 classifier may not intercept standalone P1-d2. An otherwise valid
V1 suffix paired with a malformed declared composition is corruption and may
not fall through to ordinary P1-d2. Package discriminator, not journal suffix
alone, owns the routing choice.

After the journal is covered by replacement `S_ack` or `S_truth`, direct phase
restoration uses the authenticated combined package and does not reclassify a
journal-ahead suffix.

## Retained V1, sequence and source contracts

R6's accepted corrections remain exact:

```text
DispatchAttemptRecorded V1
-> BrokerOrderObserved V1
-> BrokerTradeObserved V1
-> RequestFinalized(Completed) V1
-> allocate adjacent ACK/truth pair
-> S_ack -> S_truth -> source XACK last
```

Dispatch-only recovery appends no second dispatch and reacquires no schedule.
Order-only recovery appends only the missing trade. No pair exists before
RequestFinalized; the post-allocation/pre-ACK SIGKILL marker must equal the
final authenticated audit pair and `seq_truth = seq_ack + 1`.

## Required negatives

Every one of the 105 positive cells retains duplicate/conflict variants. R7
must additionally reject:

1. auto-ID `XADD *` in the generated-Market publication path;
2. missing reservation or XADD before reservation-package reread;
3. non-successor, repicked, malformed or overflow Redis ID;
4. stream advancement after predecessor observation;
5. marker/entry ID different from the reservation, including byte-identical E2;
6. marker missing with reserved entry present or marker present with entry absent;
7. reversed composite/P1-d2 precedence;
8. fallback to P1-d2 after `PresentInvalid`;
9. P1-d4 interception when composition is `Absent`;
10. suffix 3/4 returned as ordinary P1-d2 while composition is valid;
11. canonical field reorder, map encoding, little-endian integer, textual UUID,
    textual digest or noncanonical Redis ID;
12. any mismatch from the checked-in canonical bytes/hash fixture;
13. generation skip, reuse, overflow, downgrade or binding to a successor seal;
14. all inherited R6 journal, sequence, publication, source and XACK negatives;
15. any operational Redis DB0/VPS, FINAM or live surface opening.

## Source resumption boundary

Only independent acceptance of this exact R7 artifact may resume P1-d4 source
work. The later source slice is limited to the precommitted reservation,
explicit-ID Lua publication, package-aware routing, seven R6 owners, exact V1
continuations, canonical binding, 105 cells and duplicate/conflict variants.

R7 itself is documentation/evidence/checker/handoff only. Operational Redis
DB0/VPS, P1-e, FINAM POST/DELETE, broker dispatch, runtime-live, real orders,
partial fills, fees/slippage, replace/protective, bracket/multi-leg orders and
Generation-2 production authorization remain closed.
