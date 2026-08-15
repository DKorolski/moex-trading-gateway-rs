# Stage 8A-4 durable-composition implementation specification R1 negative inventory

The specification checker must reject all 40 mutations:

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

The harness operates only on a temporary copied tree and performs no journal,
Redis, broker or network effect.
