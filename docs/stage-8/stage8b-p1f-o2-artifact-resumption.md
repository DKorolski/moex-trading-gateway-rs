# O2 artifact resumption — preparation checkpoint

Date: 2026-09-28. Status: DEVELOPMENT IN PROGRESS, NOT AN ARTIFACT HANDOFF.

This local development branch starts at accepted-source authority successor
`cfa51fcf5be0187a2a10677dd08d8c280887faa8`. PR #9 is running fresh mandatory
checks. No merge, install or execution result is claimed by this checkpoint.

## Implemented preparation

- Rebind the source and supervisor templates to the accepted baseline07 BO-only
  profile and runtime config fingerprint. No changes to the embedded profile,
  Rust runtime, bars/session windows, trust keys or deployment units.
- Preserve source schema v2: a profile correction does not invent a new schema.
- Add a shared artifact-validation check linking both public templates to the
  exact accepted canonical profile bytes; exercise a positive control and twelve
  mutations (old High180 IDs/hashes/fingerprint, model/MR drift, schemas,
  source sentinel and account alias).
- Include the ten accepted O2 recovery regressions in the existing artifact
  witness commands. No new orchestration or governance framework.

## Completion still required

1. After required PR checks pass, synchronize by merge commit and retain its exact
   SHA/tree. Build Linux/amd64 binaries from that source, not a moving branch.
2. Replace superseded implementation refs and binary/source/template/unit hashes
   in the existing artifact manifest and its checkers/packager. Current old
   manifest hashes deliberately fail against the changed templates; this is not
   a completed or runnable artifact.
3. Run the complete revised artifact gate, negative controls and isolated linked
   witnesses, preserving Linux custody and actual operator exit evidence (runner
   failures exit 70; typed StopNotProven code 72 is not the process exit).
4. Create and verify the immutable artifact ZIP for review. Only accepted artifact
   bytes may proceed to separately authorized installation/one bounded bootstrap.

The old accepted source review and its logs remain immutable. Python template
checks are not Rust runtime execution or Linux unit execution. Docker image
preflight confirmed the existing digest resolves to Linux Rust 1.98.0; this is
the pinned artifact builder, separate from the accepted Mac/CI Rust 1.95.0.
No private inputs are used. VPS, FINAM, operational Redis, signed authority,
service starts, broker writes, O3/O4 and live micro are outside this checkpoint.
