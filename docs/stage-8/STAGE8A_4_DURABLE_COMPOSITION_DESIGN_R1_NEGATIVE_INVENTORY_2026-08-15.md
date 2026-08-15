# Stage 8A-4 durable-composition design R1 negative inventory

The fail-closed design harness executes these 24 mutations:

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
14. recovery-seal revalidation removed;
15. operator-arm revalidation removed;
16. kill-switch revalidation removed;
17. Conflict allowed to advance lifecycle;
18. replay idempotency disabled;
19. ACK allowed before durable transition;
20. retry/re-arm/resend capability enabled;
21. durable implementation opened;
22. FINAM POST/DELETE opened;
23. runtime-live opened;
24. a production Rust file changed in the design diff.

Every mutation must make the design checker fail. The harness contains no
broker, Redis or filesystem effect.
