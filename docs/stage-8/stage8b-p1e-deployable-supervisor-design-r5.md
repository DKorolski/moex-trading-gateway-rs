# Stage 8B-P1-e R5 deployable paper supervisor design correction

Status: design-only review candidate. P1-e source implementation remains
unauthorized until independent acceptance of this exact R5 package.

R5 is a direct child of held R4 commit
`98523fd009712883f73f9b5a15cb545c8e9f13ac`. The immutable accepted
predecessor remains P1-d4 closure
`c2a9e1246dfdd59f3a6297268de907dedcb19903`. The independent R4 review is
bound by SHA-256
`0432058a685317fd8aa3b262a790cb35c146ec5d0edb3ba1995e8dc1b84fcffc`.

R5 retains every accepted R4 correction and closes three P1 execution-contract
gaps and their P2 checker coverage. It changes design, matrices, checker,
negative harness, evidence, status and handoff tooling only. Production Rust,
Cargo, workflows, active systemd units, deployed config, Redis/FINAM source and
governance authority remain byte-unchanged.

## 1. Active contract V5

The only active merge contains 217 REQUIRED rows:

```text
R1  83 of 88
R2  34 of 48
R3   9 of 43
R4  49 of 56
R5  42 of 42
----------------
   217 active rows
```

Every excluded row has one exact active replacement. Semantic Authority
Registry V5 expands the execution-level inventory from 14 to 22 keys. It
retains all R4 keys and adds the main address families/private-network/endpoint
policy, bootstrap and recovery isolation, first-boot classifier disjointness,
the exact `P1d3TruthCommitted` phase set and the cancel-recovered-only
continuation.

## 2. Mode-specific systemd network contract

Identity, filesystem and credential controls remain common to all modes. The
network policy is intentionally not common.

The main `run` unit has:

```text
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6
PrivateNetwork=false
IPAddressDeny=any
IPAddressAllow=127.0.0.1/32
IPAddressAllow=::1/128
```

Application preflight additionally requires one byte-exact manifest URL:

```text
redis://127.0.0.1:6379/15
redis://[::1]:6379/15
```

Thus systemd limits the destination address while the application binds host,
port and DB. A non-loopback destination, wrong port, wrong DB, missing INET
family or `PrivateNetwork=true` fails before connection.

Bootstrap and bootstrap-recover retain:

```text
RestrictAddressFamilies=AF_UNIX
PrivateNetwork=true
Redis contact=false
```

Their first-boot operations therefore cannot contact Redis. This split is
machine-compared with Redis deployment manifest V1 and has positive tests for
both exact loopback literals and negative tests for every boundary drift.

## 3. Mandatory adopted-marker completion

R5 selects one model: a valid receipt is a durable prerequisite, but ordinary
run is not authorized until the canonical marker reaches authenticated phase
`adopted`.

The classifier evaluates every specific authenticated predicate and requires
exactly one match; it has no precedence rule. Zero or multiple matches become
`CorruptOrIdentityMismatch`, exit 66, preserving evidence.

The post-receipt states are disjoint:

| State | Marker | marker temp | receipt | Sole action |
| --- | --- | --- | --- | --- |
| `ReceiptCommittedMarkerUpdatePending` | `seal_committed` | absent | valid final | create and commit adopted-marker update |
| `SealCommittedToAdoptedMarkerTempPending` | `seal_committed` | valid `adopted` temp | valid final | rename exact temp, fsync parent, reread |
| `AdoptedCommittedRoot` | `adopted` | absent | valid final | ordinary run only |

Receipt-temp states remain separate because the final receipt is absent. The
ordinary-run commit point is adopted-marker rename plus state-parent fsync plus
reread authentication. The prior “valid receipt wins”/best-effort-marker rule
is explicitly superseded. Neither recovery state may recreate the receipt,
rerun the Ready restart, issue a second transaction or repeat an earlier phase
effect.

Transaction V4 carries eleven base classes plus the four phase-aware temp
classes. Its disjointness dimensions and exact filesystem-state fixtures are
checked pairwise.

## 4. Exact cancel-recovered routing

The Rust restart variant remains `P1d3TruthCommitted`; R5 does not invent a
variant. Outer matrix V3 admits exactly two authenticated phases for it:

```text
p1d3_s_truth
p1d3_s_cancel_recovered
```

The latter represents the accepted post-`S_cancel_recovered`, pre-source-XACK
crash frontier and starts at `s_cancel_recovered_committed`. Its sole source
continuation is `resume_stage8b_p1d3_truth_with_redis`, restricted to exact
source XACK or continuous-frontier proof. Truth replay, ACK replay, equivalent
authority reissue and a new callback are forbidden.

Operational matrix V5 has four explicit cancel-recovered cells:

- OC55: exact claimable source;
- OC56: source already acknowledged with continuous frontier;
- OC57: exact source not yet claimable;
- OC58: exact claimable source plus simultaneous due Day timer.

OC55 performs the existing wrapper’s sole reclaim and XACK-last. OC56 performs
no second XACK. OC57 remains Degraded with no fresh read. OC58 applies
`SOURCE_FIRST_TIMER_DEFERRED`: source resolution precedes timer
reclassification against the returned authenticated owner. PaperReady is
illegal before source resolution in every case.

The outer inventory is now 23 logical phase rows; the operational inventory is
56 exact cells. Every other tuple remains exit 67 before transition.

## 5. Checker and authorization boundary

The R5 checker cross-validates:

- the mode-specific network policy against the TCP-only DB15 Redis manifest;
- all first-boot specific predicates as a disjoint state partition;
- the post-receipt/pre-temp, temp-fsynced and fully adopted actions;
- both exact `P1d3TruthCommitted` phases in both matrices;
- all four cancel-recovered source/timer states and replay prohibitions;
- the retained R4 identity/custody, marker response-loss, acquisition Model B,
  linear payload, source-first and byte-exact digest contracts.

Redigested negatives include the adversarial R4-review cases: shared private
network reintroduction, non-loopback/wrong DB, receipt/temp classification
overlap, receipt-only run authority, missing/nonsense cancel-recovered phase,
generic truth/ACK replay, premature PaperReady and matrix/precedence drift.

Independent acceptance of this exact R5 design may authorize only a P1-e
source implementation against V5. It does not authorize service installation,
P1-f, operational Redis DB0/VPS, non-loopback Redis, FINAM POST/DELETE, broker
dispatch, runtime-live, real orders, partial fills or protective orders.

The inherited nonblocking hardening `0 < child_pid <= u32::MAX` remains
deferred to the separately authorized source slice.
