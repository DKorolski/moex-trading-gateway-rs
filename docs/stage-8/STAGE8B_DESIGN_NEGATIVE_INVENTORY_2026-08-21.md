# Stage 8B-D R2 negative inventory

The exact mandatory mutation set contains 50 cases:

1. accepted Stage 8A-5 ref drift;
2. accepted GOV-CI-1B ref drift;
3. design-base merge ref drift;
4. retained R1 lineage drift;
5. acceptance matrix count drift;
6. negative inventory count drift;
7. open implementation directly after R2;
8. remove one independently reviewed phase;
9. allow multiple commands or a follow-up;
10. open MARKET or protective command scope;
11. omit execution-qualified build manifest;
12. omit source/archive binding;
13. omit Cargo.lock/Toml inventory binding;
14. omit rustc/Cargo/target binding;
15. omit cargo metadata dependency graph binding;
16. omit complete profile/package/feature binding;
17. enable broker-cli legacy actual-send feature;
18. enable finam-gateway legacy actual-send feature;
19. authorize a missing or unknown feature state;
20. permit an alternate real transport path;
21. remove immutable toolchain/action pin prerequisite;
22. replace HMAC with plain SHA-256;
23. permit raw account ID in Git/handoff;
24. permit operator key in Git/handoff;
25. weaken operator key below 256 bits;
26. remove account domain separation or key generation binding;
27. allow account normalization or non-constant-time verification;
28. allow account-binding fallback;
29. make arm cloneable/serializable/default or reusable;
30. reconstruct or mint a second arm after restart;
31. omit fresh build/feature/account/API preflight match;
32. let caller snapshots authorize;
33. ignore kill switch or single-owner state;
34. allow ambiguity or unresolved lifecycle;
35. enter transport before durable attempt/fsync/seal;
36. treat Redis as execution authority;
37. retry or resend after possible send;
38. classify possible-send timeout/malformed 2xx as definitely not sent;
39. let empty account-wide rows prove flat;
40. let broker truth rewrite durable identity;
41. remove one safe-closure state;
42. accept a residual/unknown/conflict state as safe;
43. weaken any Stage8BClosedSafe predicate;
44. automatically cancel/flatten residual state;
45. allow new arm while residual/unknown/conflict remains;
46. reduce Stage 11 normal sessions below three or remove consecutive-day rule;
47. let recovery replace a normal session or preserve count after blocking fix;
48. allow semantic divergence, CLI overrides or FINAM execution during Stage 11;
49. permit incomplete reachable-action coverage or silent action/quantity rewrite;
50. open Stage 8B-S, Stage 8B execution, FINAM/Redis/dispatch/runtime-live/real orders or Stage 12.
