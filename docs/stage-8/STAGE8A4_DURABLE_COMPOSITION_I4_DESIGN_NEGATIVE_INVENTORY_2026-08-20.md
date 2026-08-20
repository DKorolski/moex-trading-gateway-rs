# Stage 8A-4 I4 design negative inventory

The design gate must reject each mutation independently:

1. accepted I3 R6 predecessor removed or changed;
2. I3 R6 review digest removed or changed;
3. terminal authority allowed from an I3 receipt alone;
4. incomplete V2/suffix accepted;
5. complete-but-uncovered history accepted;
6. missing `RequestFinalized` accepted;
7. either reconciliation hold mapped to terminal ACK;
8. CANCEL Working mapped to terminal ACK;
9. PLACE terminal rejected mapped to recovered success;
10. CANCEL Filled without `ExecutionObserved` accepted;
11. CANCEL Rejected/Expired without `AlreadyTerminalNonExecution` accepted;
12. CANCEL Cancelled without `Canceled` accepted;
13. current readiness inferred from terminal ACK;
14. `StopRequested` mapped ready;
15. stale/unreadable control mapped ready;
16. stale broker truth or readiness mapped ready;
17. unknown/orphan account safety mapped ready;
18. I3 post-effect control reused as current readiness;
19. duplicate derivation permits second finalization or append;
20. canonical terminal identity includes unrelated later seal generation;
21. public constructor or field access added to an authority/facade;
22. authority/facade becomes Clone/Debug/Serialize/Deserialize;
23. Redis ACK/XACK or publication opened;
24. FINAM POST/DELETE, broker dispatch, retry/re-arm, runtime-live or real orders opened.
