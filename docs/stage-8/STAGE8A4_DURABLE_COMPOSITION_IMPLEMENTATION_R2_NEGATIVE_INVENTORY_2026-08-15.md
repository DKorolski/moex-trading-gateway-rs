# Stage 8A-4 durable-composition implementation specification R2 negative inventory

The R2 checker retains every R1 concept and must reject all 57 mutations:

1. accepted Design R2 ref drift;
2. accepted review hash drift;
3. forged accepted status;
4. specification-only flag disabled;
5. production Rust declared changed;
6. V1 bytes made mutable;
7. V1 semantics made mutable;
8. source-evidence digest smuggling enabled;
9. historical rewrite enabled;
10. mixed replay disabled;
11. unknown V2 skip enabled;
12. V2 event kind changed;
13. stable key field removed;
14. nonce added to stable key;
15. exact lookup evidence removed from V2;
16. suffix manifest removed from V2;
17. query account binding removed;
18. query BrokerOrderId binding removed;
19. response timing removed;
20. transition no longer first batch record;
21. per-record durability disabled;
22. covering seal allowed before suffix completion;
23. restart stable-key lookup disabled;
24. restart appends a second transition;
25. same-key different-payload accepted;
26. pre-append seal fingerprint removed;
27. S1 frontier coverage disabled;
28. S1 reread validation disabled;
29. expired arm blocks reconciliation append;
30. `StopRequested` blocks reconciliation append;
31. stale/unreadable kill switch blocks reconciliation append;
32. reconciliation send enabled;
33. PLACE `ExactWorking` left unresolved;
34. PLACE terminal rejection finalized Completed;
35. CANCEL `ExactWorking` finalized;
36. CANCEL terminal filled mapped non-execution;
37. hold terminal ACK/XACK enabled;
38. terminal ACK allowed without covering seal;
39. I1 durable writer opened;
40. production Rust path added to specification diff.
41. V2 lifecycle sequence omitted;
42. V2 previous record ID omitted;
43. V2 durable request identity omitted;
44. V2 event kind is merged into or mutates V1 event-kind semantics;
45. unknown record schema version is skipped;
46. failed V2 decode falls back to V1;
47. mixed replay ignores V2 sequence/frontier;
48. V2 directly applies suffix semantics or finalizes the request;
49. pending-batch replay loses stable key or suffix manifest;
50. suffix manifest binds payload hash but not full canonical record hash;
51. same payload with changed source evidence is accepted;
52. same payload with changed previous/causal identity is accepted;
53. BrokerOrderId is invented for V1 order compatibility projection;
54. trade BrokerOrderId is invented for V1 trade compatibility projection;
55. client-linked trade without BrokerOrderId is dropped from V2;
56. successful exact lookup loses its returned observation;
57. immutable V1 canonical golden bytes are allowed to change.

The harness operates only on a temporary copied tree and performs no journal,
Redis, broker or network effect.
