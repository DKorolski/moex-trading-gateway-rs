# Stage 8A-4 durable-composition design R2 negative inventory

The fail-closed design harness retains all 24 R1 mutation classes and adds 14
R2 semantic mutations, for 38 cases total:

1. accepted reducer ref drift;
2. accepted reducer review hash drift;
3. design forged accepted;
4. production Rust change declared allowed;
5. public diagnostic promoted to authority;
6. authoritative result made public;
7. authoritative result made caller-constructible;
8. partial-identity merge enabled;
9. documented not-found promoted to no-match;
10. unavailable promoted to no-match;
11. `ProvenNoMatch` enabled;
12. unknown-status safety count removed;
13. orphan safety count removed;
14. recovery-seal precondition removed;
15. operator-arm provenance validation removed;
16. kill-switch validation removed;
17. Conflict allowed to advance lifecycle;
18. replay idempotency disabled;
19. derived publication allowed before durable transition;
20. retry/re-arm/resend capability enabled;
21. durable implementation opened;
22. FINAM POST/DELETE opened;
23. runtime-live opened;
24. a production Rust file changed in the design diff;
25. stable transition key replaced with random nonce;
26. mutable post-append generation added to transition identity;
27. post-append covering seal requirement removed;
28. ACK allowed before covering seal validation;
29. append-before-seal crash recovery reruns append;
30. expired operator arm blocks reconciliation append;
31. `StopRequested` blocks reconciliation append;
32. `DocumentedNotFound` downgraded to `NotAttempted`;
33. `Unavailable` downgraded to `NotAttempted`;
34. Conflict hold publishes terminal ACK;
35. StillUnknown hold publishes terminal ACK/XACK;
36. transition vocabulary mutated to `RetryAllowed`;
37. `DecodeFailure` downgraded to `NotAttempted`;
38. `Stale` downgraded to `NotAttempted`.

Every mutation must make the exact-pinning checker fail. The harness contains
no broker, Redis or filesystem effect outside its temporary copied tree.
