# Stage 5F-b — fixture, input and fingerprint contract

Status: fixture/input contract complete; source outputs not characterized
Version: 1
Target: IMOEXF Hybrid / `imoexf_primary_riskgate_high180_lb120`
Mode: canonical final live M10, paper-only, no send

## Purpose

This contract freezes deterministic Stage 5F scenario inputs before any Stage
5F callback is invoked. It inherits the source-reachability decisions from
commit `d71af08804c9fc44c4f056cfa24396386d9ed94d` and preserves the sole B3F
callback/settlement route.

There are exactly 34 scenario records covering exactly 16 official groups.
State and riskgate seeds are stored in shared catalogs and are selected by a
stable `seed_id`; every scenario binds both the catalog path and its SHA-256.
This avoids 34 copies of a large persistence envelope while retaining exact,
reviewable inputs.

## Honest characterization boundary

The external completion proposal both forbids callback invocation in Stage
5F-b and asks Stage 5F-b to contain final post-callback fingerprints and intent
vectors. Those requirements are circular: a source-exact output cannot be
known without either invoking the source callback or independently
reimplementing the strategy.

This project does neither. Version 1 uses a three-state output lifecycle:

1. `pending_source_characterization`
   - Stage 5F-b state;
   - all fingerprint/vector output values are JSON null;
   - the record is input evidence only and cannot pass an execution acceptance
     gate.
2. `candidate_source_characterized`
   - produced in Stage 5F-c by one authorized source callback and one canonical
     settlement attempt;
   - stored separately from the input catalog;
   - cannot be used as an expected value in the same run.
3. `frozen_golden`
   - created only by an explicit later freeze commit after candidate review;
   - accepted rows require complete pre/post fingerprints and ordered vector;
   - blocked/terminal rows retain null accepted post-state by contract.

This is deliberately stricter than filling placeholders with guessed hashes.

## Scenario catalog schema

Catalog path:

```text
tests/fixtures/stage5/stage5f/v1/scenarios/atomic-hybrid-scenarios.json
```

The top-level object contains exactly:

```text
schema_version          exact JSON integer 1
fixture_kind            stage5f-atomic-hybrid-scenario-catalog
characterization_policy exact policy object
records                 array of exactly 34 records
```

Each record contains exactly:

```text
schema_version
scenario_id
row_id
group_id
case_id
target
bar
clock
pre_state
riskgate
expected
owning_test
```

Required target binding is the full synthetic test scope:

```text
strategy_id  hybrid_imoexf
account_id   ACC_TEST_0001
instrument   IMOEXF / IMOEXF@RTSX / Moex / Futures
profile      imoexf_primary_riskgate_high180_lb120
paper_only   true
```

The bar is `Live`, final and exactly 600 seconds. OHLCV values are finite
canonical decimal strings; timestamps are explicit RFC3339 UTC strings. Event,
callback and lifecycle timestamps are monotonic and no test may read the wall
clock to replace them.

`pre_state` and `riskgate` contain only `catalog_path`, `catalog_sha256` and
`seed_id`. Absolute paths, traversal and unbound fixture files are rejected.

## State seed contract

State seeds describe the accepted Stage 5D restored input, not a second
runtime implementation. Stage 5F-c must materialize them through the existing
Stage 5D/Stage 5E capability chain. The catalog covers fresh flat, BO/MR open,
owner-bound, duplicate-bar, post-cleanup, pending and deferred states.

Decimal values are strings so JSON numeric coercion cannot alter their bits.
Synthetic UUIDs and cycle identifiers are allowed inside local input fixtures;
exported evidence must contain only domain-separated hashes for cycle/comment
or broker-native identifiers.

## Riskgate seed contract

The riskgate catalog defines four input states:

- complete `normal_append` authority;
- missing authority;
- inconsistent authority;
- materialization-integrity terminal.

`RiskGateMode::Enforced` is not an allowed seed. `normal_append` updates shadow
and ledger semantics but does not become entry enforcement.

## Fingerprint contract

Stage 5F transition evidence uses only the existing complete-state algorithm:

```text
stage5c_state_fingerprint
  = sha256(serde_json::to_vec(StrategyState))
```

The persistence projection algorithm
`stage5c_semantic_payload_fingerprint` remains a separate Stage 5D binding.
The checker rejects any claim that the two digest domains must be equal.

## Ordered intent projection v1

The future frozen projection is ordered and typed. Every element contains:

```text
ordinal
settled_strategy_request_id
intent_class
base_action
route_symbol
owner
role
side
cycle_id_domain_sha256
quantity_f64_bits_be
price_f64_bits_be
trigger_f64_bits_be
fill_f64_bits_be
stop_end_unix_time
broker_order_id_domain_sha256
broker_stop_id_domain_sha256
comment_present
comment_domain_sha256
check_duplicates
condition_flags
```

Vector order is never sorted. Floating-point evidence is 16-character
lowercase hexadecimal from `f64::to_bits()` in big-endian display order.
Non-finite values and negative zero fail. The vector hash domain is
`moex.stage5f.ordered-intent-vector.v1`.

## Parsing and fail-closed rules

The checker rejects:

- duplicate or unknown JSON keys;
- missing records, duplicate row/scenario/test IDs or order drift;
- `bool` where an integer is required, floats where exact integers are
  required, noncanonical decimal strings and invalid timestamps;
- catalog path/hash/seed mismatches;
- wrong target, profile, bar origin/finality/timeframe or paper mode;
- non-null output goldens while status is pending;
- frozen accepted output with any missing fingerprint/vector field;
- an extra JSON file below the versioned fixture root;
- changes to the frozen Stage 5C/B3F source in this contract-only substage.

## Controlled observer

`stage5f-controlled-observation-extension.json` is design-only. No observer,
callback invocation or Rust source modification is part of Stage 5F-b. The
observer can be implemented only in Stage 5F-c and only in the three approved
test-only regions.

## Closed surfaces

Redis, FINAM, HTTP POST/DELETE, dispatch, broker send, runtime-live,
ACK/order/position/timer/restart feedback and protective-order implementation
remain closed.
