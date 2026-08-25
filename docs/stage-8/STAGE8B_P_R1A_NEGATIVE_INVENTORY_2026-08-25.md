# Stage 8B-P R1A negative inventory

The R1A harness adds these 50 mutations to the inherited R1 48/48 matrix:

1. R1 authority digest changed;
2. freshness authority digest changed;
3. network authority digest changed;
4. full execution-build identity changed;
5. weaker execution-build reconstruction enabled;
6. process-boot field omitted;
7. cross-boot substitution allowed;
8. restart reuse allowed;
9. common manifest field added;
10. operation discriminator expanded;
11. unknown manifest fields allowed;
12. irrelevant variant fields allowed;
13. flat PLACE/CANCEL union allowed;
14. price/cancel-target conflation allowed;
15. PLACE field omitted;
16. CANCEL field omitted;
17. PLACE instrument changed;
18. PLACE type changed to MARKET;
19. PLACE TIF changed;
20. PLACE quantity changed;
21. canonical decimal grammar weakened;
22. noncanonical decimal accepted;
23. decimal overflow accepted;
24. notional exceedance accepted;
25. pre-attempt notional check removed;
26. pre-K4 notional recheck removed;
27. exact CANCEL broker order ID removed;
28. same-lifecycle CANCEL removed;
29. currently-working proof removed;
30. account-wide CANCEL selection enabled;
31. endpoint identity requirement removed;
32. network policy requirement removed;
33. endpoint formula requirement removed;
34. network TLS disabled;
35. exact host changed;
36. redirect enabled;
37. proxy enabled;
38. automatic retry enabled;
39. PLACE method changed to DELETE;
40. CANCEL route changed to PlaceOrderV1;
41. caller-selected freshness budget enabled;
42. runtime cross-source skew widened;
43. API snapshot age widened;
44. issued arm metadata permitted before K1;
45. R2 allowed to issue an arm;
46. cached R2 evidence accepted as K2;
47. R2 evidence accepted for K1/K2 freshness;
48. R2 evidence carried into XE current truth;
49. post-arm K2 fresh reread removed;
50. broker/transport authorization changed from `NOT_ISSUED`.

Every mutation must fail through the same R1A checker. The R1A gate separately
replays the inherited R1 checker and all inherited 48 negative cases, giving
98/98 total declared mutations.
