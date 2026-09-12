# Stage 8B-P1-e I1A R2 — semantic identity correction

Status: `DESIGN_SEMANTIC_IDENTITY_CORRECTION_REVIEW_CANDIDATE_IMPLEMENTATION_NOT_AUTHORIZED`.

Foundation R2 at `37088964e50c0ceb4d82887a30c32103110749b0` is accepted and
unchanged. This additive correction closes the sole P1 remaining on the held I1A R1
design at `7686c93eb9f38a0124d2ae558a80b78f51c99f8f`. It does not open
production source code or any operational surface.

## Exact semantic identity

`Stage8bP1eScheduleSemanticIdentityV1` is a canonical object containing exactly:

- instrument symbol/broker symbol/exchange/market/MIC/board/tick size;
- registry version and identity hash;
- trading day, Europe/Moscow timezone, and 600-second timeframe;
- ordered normalized session type/start/end triples;
- Stage4 semantic evidence kind/state and exact boundary proof when Closed.

It excludes transport generation/sequence, publication and observation times,
freshness expiry, raw/normalized payload hashes, Stage4 report bytes/hash, payload hash,
signature, and envelope hash. Therefore a heartbeat can refresh authenticated evidence
without manufacturing a semantic change.

The identity is encoded with `strict-canonical-json-subset-v1`. Its hash is:

```text
SHA256(
  UTF8("moex.stage8b.p1e.schedule-semantic-identity.sha256.v1")
  || 0x00
  || canonical_semantic_identity_bytes
)
```

The result must equal envelope `schedule_semantic_sha256`. Envelope v3 carries the
canonical semantic projection explicitly and the verifier must cross-validate every
projected field against the independently validated payload before signature
acceptance. Producer and consumer therefore cannot choose different field sets.

## Producer revision rule

Revision state is scoped to one `source_generation` and persisted together with
publication sequence and the last semantic hash.

- First publication uses semantic revision 1.
- Same semantic hash as the immediately prior publication retains the exact revision.
- Different semantic hash increments revision by exactly one.
- Publisher restart must restore the exact durable tuple before publishing; reset or
  guessed state is fail-closed.

`tradability/open → day_boundary/closed` changes evidence kind/state and boundary proof,
so it changes the hash and increments revision even when normalized sessions are
unchanged. A trading-day transition also changes the hash and revision. A freshness-only
heartbeat increments publication sequence but retains semantic hash and revision.

## Consumer progression and route link

The consumer retains the snapshot rule from I1A R1:

- lower semantic revision is rollback;
- equal revision/equal hash continues transport validation;
- equal revision/different hash is terminal conflict;
- higher revision continues transport validation without requiring observation of
  intermediate snapshot revisions.

The linked model computes the semantic bytes/hash, derives or checks producer revision,
runs transport progression, and only then evaluates route eligibility. It proves:

- Open revision 7 followed by Closed revision 8 is accepted and can reach Day expiry
  when the exact book/M10/boundary predicates hold;
- an unchanged heartbeat keeps revision 7 and does not produce a false conflict;
- a trading-day change increments revision;
- changed hash with retained revision remains terminal conflict;
- revision rollback remains blocked;
- accepted Closed evidence still cannot authorize Market or Working evaluation.

## Coverage classification and next boundary

The inherited R1 model has 41 cases; 14/41 binding/timer cases are explicit table
lookups. They are design inventory, not execution evidence. The eight R2 cases are
linked computed model cases, but likewise are not production source evidence.

After independent I1A R2 acceptance, the prospective implementation scope may open for
the Stage5E facade, signed adapter, V4 binding/recovery and fixture-backed tests. That
source review must execute actual state transitions, append/seal/effect counters,
crash/replay behavior and latch checkpoints. Redis/VPS activation, private-key install,
FINAM POST/DELETE, broker dispatch, runtime-live and real orders remain closed.
