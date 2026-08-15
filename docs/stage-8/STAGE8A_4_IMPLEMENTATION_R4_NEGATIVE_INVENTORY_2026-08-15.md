# Stage 8A-4 implementation R4 negative inventory

The mandatory R4 harness retains all 55 R3 mutations and executes 58 exact
fail-closed mutations in total:

1. accepted design ref drift;
2. accepted review hash drift;
3. implementation status forged accepted;
4. canonical truth type drift;
5. implementation kind drift;
6. public input constructor enabled;
7. source-specific completeness disabled;
8. exact lookup replaces account safety;
9. deterministic interval split disabled;
10. trade dedup disabled;
11. lifecycle/fill collapsed;
12. `ProvenNoMatch` enabled;
13. retry authority enabled;
14. send authority enabled;
15. focused test count reduced;
16. compile-fail count reduced;
17. durable apply opened;
18. ACK publication opened;
19. Redis live consumer opened;
20. broker dispatch opened;
21. FINAM POST/DELETE opened;
22. same-request resend opened;
23. runtime-live opened;
24. real orders opened;
25. Stage 8A-5 opened;
26. Stage 8B opened;
27. production module declaration removed;
28. durable binding removed from admission hash;
29. policy binding removed from admission hash;
30. saturation comparison weakened;
31. trade conflict result weakened;
32. exact disagreement result weakened;
33. unknown status made terminal;
34. retry diagnostic changed to true;
35. send diagnostic changed to true;
36. source-specific wrapper removed;
37. split-depth guard removed;
38. exact lookup injected into account-wide list;
39. forbidden network token inserted;
40. historical reconciliation authority imported.
41. admission durable binding removed;
42. admission policy binding removed;
43. reducer context cross-pair check removed;
44. reducer policy cross-pair check removed;
45. canonical payload equality weakened;
46. exact GET request timing wrapper removed;
47. selected-order identity validator removed;
48. supporting-trade secondary identity conflict weakened;
49. material trade summary replaced with raw representative serialization;
50. non-exact request/admission binding removed.
51. durable broker identity removed from supporting-trade classification;
52. durable client identity removed from supporting-trade classification;
53. unrelated-trade early return weakened;
54. request ID removed from pre-admission failure binding;
55. leading current-status authority regressed to Design R2 pending.
56. both durable identities widened into the selected-order support predicate;
57. durable broker identity alone widened into the support predicate;
58. durable client identity alone widened into the support predicate.

Every mutation must make `stage8a4_implementation_check.py` fail. Merely
mentioning a forbidden token in this inventory is not production activation;
the checker scans the Rust implementation independently.
