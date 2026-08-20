# Stage 8A-4 I4 design R3 negative inventory

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
25. CANCEL target order client ID is used as ACK client ID;
26. an ID-less selected V2 order erases durable replay-established trade `B1`;
27. current broker truth fills or changes a missing durable ACK broker ID;
28. `Utc::now()` or publication `received_ts` changes durable ACK facts on restart;
29. a second unrelated request-level ACK identity domain is introduced;
30. seal generation, current checkpoint or readiness enters stable ACK identity;
31. caller-supplied public readiness snapshots are accepted;
32. operator arm or `Stage8ExecutionCapability` is minted to derive readiness;
33. readiness from another account, instrument, strategy or runtime is accepted;
34. any active ExactWorking order is normalized to generic `Ready`;
35. expired readiness or changed source revision is accepted;
36. I4 advances, writes or repairs a recovery seal;
37. I4 appends Stage6, V2, suffix or finalization records;
38. the design forbids the read-side S1/replay/current-source checks it requires.
39. terminal authority remains `pub(crate)` and cannot cross crates;
40. terminal authority gains a public constructor;
41. terminal authority gains caller-settable public fields;
42. terminal authority becomes `Clone`, `Serialize` or `Debug`;
43. `runtime-durable-service` gains a dependency on `finam-gateway`;
44. raw public terminal facts replace the opaque authority;
45. `finam-gateway` independently validates/reconstructs Stage7B seal authority;
46. FINAM-private ACK facts, readiness evidence or facade becomes public.
