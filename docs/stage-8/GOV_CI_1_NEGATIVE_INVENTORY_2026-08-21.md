# GOV-CI-1B negative inventory

1. GOV-CI-N01 — replace the current gate invocation with an `echo` no-op.
2. GOV-CI-N02 — replace current debug tests with an `echo` no-op.
3. GOV-CI-N03 — replace current release tests with an `echo` no-op.
4. GOV-CI-N04 — replace current doctests with an `echo` no-op.
5. GOV-CI-N05 — replace current all-feature clippy with an `echo` no-op.
6. GOV-CI-N06 — neutralize the no-Redis smoke while retaining its marker.
7. GOV-CI-N07 — neutralize the Redis shadow smoke while retaining its marker.
8. GOV-CI-N08 — comment out the current authority checker in the gate.
9. GOV-CI-N09 — comment out the current negative harness in the gate.
10. GOV-CI-N10 — replace immutable Stage 8A-5 replay with an `echo` no-op.
11. GOV-CI-N11 — replace a mandatory gate command through a shell wrapper.
12. GOV-CI-N12 — change the immutable accepted Stage 8A-5 replay ref.
13. GOV-CI-N13 — restore the historical forbidden scanner as current authority.
14. GOV-CI-N14 — add an unapproved active GitHub workflow.
15. GOV-CI-N15 — reactivate `pull_request_target` on the historical Stage-5 workflow.
16. GOV-CI-N16 — modify the bound current authority checker.
17. GOV-CI-N17 — modify the bound current negative harness.
18. GOV-CI-N18 — modify the bound handoff maker.
19. GOV-CI-N19 — add a production Rust file without authority rotation.
20. GOV-CI-N20 — default-enable `m3j16-actual-one-shot` and recompute production manifest.
21. GOV-CI-N21 — enable the Stage-6 Redis command consumer accessor and recompute manifest.
22. GOV-CI-N22 — enable the Stage-6 runtime-live accessor and recompute manifest.
23. GOV-CI-N23 — enable the Stage-6 broker-dispatch accessor and recompute manifest.
24. GOV-CI-N24 — enable the Stage-6 real-orders accessor and recompute manifest.
25. GOV-CI-N25 — set Stage 8B-S authorization true.
26. GOV-CI-N26 — replace the accepted Stage 8A-5 ref inside the authority record.
27. GOV-CI-N27 — restore a historical Stage-5D checker as current authority.
28. GOV-CI-N28 — replace the immutable checkout action SHA with a mutable tag.
29. GOV-CI-N29 — change the immutable checkout action SHA.
30. GOV-CI-N30 — replace the immutable Rust action SHA with a mutable tag.
31. GOV-CI-N31 — change the immutable Rust action SHA.
32. GOV-CI-N32 — replace the exact Rust release with `stable`.
33. GOV-CI-N33 — change the exact Rust release.
