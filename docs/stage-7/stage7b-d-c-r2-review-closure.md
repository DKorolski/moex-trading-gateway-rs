# Stage 7B-d-c-R2 review closure

Status: independently accepted and closed at
`2b6371adb905654e0ddd8b6714159bcef737b577` on 2026-08-14.

Rejected predecessor:
`9b98c360e1153e79971b5935d03fd0a0bdd1f4f4`.

R2 is limited to the marker-only terminal-history P1 found in R1 review. It
does not reopen the three R1 closures, Stage 6, d-a seal authority, d-b Lua
ordering, B-066/B-068, or any FINAM/live surface.

## Pre-admission publication guard

For a request absent from Stage 6, `Stage7bRedisService` now reads the d-b
request marker before profile/policy classification and before any provider
call. `Stage7bCanonicalRequestPublicationEvidence` is publication-only and
opaque. It is accepted only when the marker and canonical ACK stream entry
agree on request ID, stable canonical command SHA, terminal ACK identity,
canonical output stream/ID and canonical publication class.

- no marker: normal Stage 6 path may continue;
- marker plus changed command SHA: source remains PEL conflict, with no
  Stage 6/provider/ACK/DLQ/XACK effect;
- marker plus exact command SHA: owner revalidates seal, checkpoint and Stage
  6 request absence, then the existing Lua publishes duplicate ACK plus XACK;
  no Stage 6 lifecycle is created.

The marker schema is additively hardened with `canonical_command_sha256` and
`canonical_output_stream`. A marker lacking these fields is not migrated or
treated as absent: it blocks fail-closed for operator inspection.

## Evidence

Real Redis restart tests cover changed marker-only identity, exact rejected
command replay, and a prior profile rejection that becomes profile-matching
after restart. They assert zero provider calls, byte-identical Stage 6
journal, expected PEL/ACK/DLQ state and an unchanged canonical request marker.
An additional schema test proves legacy/incomplete marker rejection.

The d-c mutation matrix has 40 cases, including all seven R2 regressions from
the review. Independent review found no remaining P0/P1 blocker and opened
only Stage 7B-e aggregate closure. All real execution surfaces remain closed.
