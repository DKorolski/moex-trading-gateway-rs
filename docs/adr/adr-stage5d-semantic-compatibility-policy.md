# ADR: Stage 5D semantic compatibility and migration policy

Status: accepted for Stage 5E/6 entry criteria.

## Decision

Long-lived Stage 5D persistence compatibility must be based on immutable
semantic compatibility identifiers, not on a mutable generic label alone.

Future production snapshots must bind at least:

- canonical schema version;
- semantic projection digest;
- migration epoch;
- source/runtime compatibility set;
- strategy/account/instrument/config/profile identity.

Compatibility changes require an explicit migration table or a new ADR. A
generic compatibility string such as `stage5d_runtime_semantic_compatibility_v1`
is acceptable only for the current no-I/O reviewed slice and must not become the
sole production compatibility authority.

## Consequence

Stage 5D aggregate closure r2 remains evidence/governance-only. Stage 5E/6
entry criteria must include immutable semantic compatibility binding or an
explicit migration plan.
