# Stage 8A-4 I2 negative inventory

The checker and mutation harness must reject all of these changes:

1. predecessor or review hash drift;
2. I2 marked accepted before independent review;
3. private outcome exported;
4. private outcome made Clone, Debug or Serialize;
5. public diagnostic accepted by the builder;
6. candidate exported or made Clone/Serialize;
7. stable-key domain drift;
8. random/mutable value added to stable key;
9. any of the four CAS fields removed;
10. V2 no longer first or sequence no longer contiguous;
11. suffix full-record hash removed;
12. DocumentedNotFound changed to exact/no-match;
13. Unavailable, DecodeFailure or Stale changed to Exact;
14. CANCEL Working receives finalization;
15. CANCEL target is taken from an observation instead of durable identity;
16. fabricated broker order ID appears;
17. trade suffix accepts a mismatched or absent broker order ID;
18. journal append/CAS/seal API or backend import appears;
19. ACK/readiness, Redis or FINAM surface appears;
20. broker dispatch, runtime-live or real-order authority appears.
21. failed exact lookup variants lose their private production producer;
22. an attempted exact failure normalizes to `NotAttempted`;
23. CANCEL TerminalRejected maps to `Rejected`;
24. CANCEL TerminalExpired maps to `Rejected`;
25. canonical orphan summary is replaced by a correlation-ID shortcut;
26. filled-without-trade orphan coverage is removed;
27. trade projection is suppressed when selected order has no broker ID;
28. two distinct material-trade broker IDs are silently accepted.
