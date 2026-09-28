# Stage 8B-P1-e I1 first-boot correction contract v2

Status: correction review candidate after the HOLD of `21fda88`.

Accepted predecessor: `8360c4701b6abbe75ced988cf8dd2d74487e1846`.
The rejected/HOLD checkpoint remains historical and does not become a new
baseline.

## Wire compatibility decision

The design-pinned production facade name
`build_stage8b_p1_first_boot_source_v1` remains unchanged. The authenticated
wire document is upgraded to schema/domain v2 and the unsafe v1 wire document
is rejected. The v1 schema file is retained only as historical review evidence.

## Candidate identity

The candidate now carries the exact accepted P1 canonical M10 material:

- operational identity from validated supervisor configuration;
- canonical OHLCV and exact M10 open/close milliseconds;
- ten contiguous M1 source identities, payload hashes and Redis IDs;
- supplied Redis ID, semantic ID and payload hash.

Before any callback or export, the durable-service boundary invokes
`build_stage8b_p1_canonical_m10`, reparses its bytes with
`parse_stage8b_p1_canonical_m10`, and requires all three supplied envelope
identities to match. Only the recomputed semantic ID crosses into runtime-core
as `validated_candidate_semantic_id_sha256`. That value binds the snapshot ID
and persisted event watermark.

## Complete History evidence

`history_coverage` is a config-bound explicit session-window authority. Every
session declares one or more disjoint Moscow-date M10 close-time windows. The
parser canonical-hashes the authority and requires an exact ordered equality
between all expected close timestamps and all History bars. Missing internal
bars, duplicates, out-of-window bars, truncated tails and dates without a
declared complete window fail before composition or durable-root creation.

The behavioral evidence uses 121 full weekday model sessions with 88 M10 bars
per session (10,648 bars total), day transitions and 120 non-zero High180
shadow outcomes (`1.4` points, one trade each). Supplied observations are still
cross-validated against the accepted source High180 kernel; no external ledger
or materialized state is accepted.

No universal exchange-calendar bar count is introduced. A source producer must
declare the exact intervals applicable to every captured session, including
explicit breaks as separate windows. The configured whole-bundle SHA-256 binds
that authority to the deployment input.

## F00 filesystem boundary

The fixed production path remains non-configurable. The loader validates
pre-open and opened metadata, uses `O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK`, and
requires a regular single-link file with exact owner/group, mode no wider than
`0640`, bounded length and unchanged inode/identity/length/mode/mtime/ctime
through the complete read.

Private Unix tests cover a regular positive source, symlink, hardlink, wrong
mode, owner, group, oversize input, change/incomplete read and bounded FIFO
failure.

## Still closed

This correction does not open transaction V5, receipt V2, owner-loop/process
supervision, operational Redis DB0/DB15, VPS activation, FINAM POST/DELETE,
broker dispatch, runtime-live or real orders.
