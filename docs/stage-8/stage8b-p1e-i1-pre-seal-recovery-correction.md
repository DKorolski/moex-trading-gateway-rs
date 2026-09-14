# Stage 8B-P1-e I1 pre-seal recovery freshness correction

Status: correction candidate for `P1-PSR01`. Accepted source baseline:
`5e2e157e032406fdbb9047c33c641f5973514504`. Held predecessor:
`be6707391dc53327fa3a29a40836d24f71eca850`.

The correction separates admission of a new first-boot transaction from
recovery of an already authenticated V5 transaction. It does not expand the
owner loop or any operational transport surface.

## Fresh admission remains unchanged

`build_stage8b_p1_first_boot_source_v1` and the ordinary F00 parser retain the
exact 300-second broker-truth maximum age. A new first boot is accepted at age
300 seconds and rejected at age 301 seconds. The correction does not extend
the TTL, rewrite `trusted_now`, backdate evidence or accept a different source
bundle.

## Administrative recovery authority

`recover_stage8b_p1e_first_boot_pre_seal_administrative_v5` accepts only:

- `remove-marker-temp`;
- `quarantine-root` for RootPublished and JournalDurable incomplete roots;
- `finalize-quarantine`.

This entry point has no `Stage8bP1ePreparedFirstBootV1`, source bundle or
trusted-clock argument. Before its first mutation it requires:

- the exact validated operational identity and runtime fingerprint;
- an HMAC-authenticated durable marker or marker-temp;
- a transaction ID recomputed from the marker's identity, source hash, source
  generation and bootstrap-attempt generation;
- the exact typed selector action;
- a fresh full V5 filesystem classification.

The existing Prepared-based function remains as a compatibility entry point,
but delegates these three actions immediately to the same administrative
implementation. Production recovery does not need to reconstruct F00 for
these actions.

## Historical continuation policy

The four continuation actions authenticate the durable marker before reading
the historical fixed-path source. The marker yields an opaque crate-private
binding for the exact original source bundle SHA-256, generation, history
hash, riskgate-observation hash and candidate semantic ID. Runtime profile,
source-plan, operational identity and transaction-ID self-binding are checked
again.

Only the age predicate changes from `FreshAdmission` to
`HistoricalRecovery`; all source bytes and semantic hashes remain exact. A
new or modified bundle cannot continue the old transaction. If the original
bundle is missing, unreadable or byte-different, continuation fails before
mutation. The three safe administrative actions remain independently
available, so an incomplete root can still be quarantined and retained.

## Executable evidence

The correction tests prove:

- ordinary admission succeeds at broker-truth age 300 and fails at 301;
- remove-marker-temp, quarantine at both incomplete frontiers and
  finalize-quarantine remain available after a simulated 30-day downtime;
- exact historical continuation succeeds at age 301 and after 30 days;
- changed source bytes fail without any filesystem mutation;
- the existing exact-selector, all-continuation, generation and response-loss
  regressions remain green.

## Closed boundary

The deployable owner loop, installed bootstrap-recover composition and release
process SIGTERM/SIGKILL/restart matrix remain later I1 slices. Operational
Redis DB0/DB15, VPS activation, FINAM POST/DELETE, broker dispatch,
runtime-live and real orders remain closed.
