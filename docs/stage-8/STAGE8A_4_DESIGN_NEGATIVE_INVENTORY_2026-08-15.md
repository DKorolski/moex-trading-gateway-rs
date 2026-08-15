# Stage 8A-4 design R2 negative inventory

The design gate inherits all 48 R1 mutation classes and rejects 20 additional
R2 classes, for exactly 68. A later reviewed revision may increase the count
but must not silently replace or reduce it.

1. Change the accepted Stage 8A-3 predecessor.
2. Change the accepted Stage 8A-3 review SHA-256.
3. Mark the package as implementation rather than design-only.
4. Claim production reconciliation is already implemented.
5. Remove orders from required broker truth.
6. Remove trades from required broker truth.
7. Remove positions from required broker truth.
8. Remove the instrument registry from required broker truth.
9. Reorder exact ClientOrderId behind BrokerOrderId.
10. Reorder shape/time correlation before an exact identity.
11. Remove `Conflict` from the outcome algebra.
12. Remove `StillUnknown` from the outcome algebra.
13. Make trades authoritative order selectors.
14. Make position authoritative order selectors.
15. Open `ProvenNoMatch`.
16. Open same-request retry or automatic resend.
17. Open network/FINAM order transport.
18. Open Redis-live/runtime-live/real-order execution.
19. State that empty truth proves no match.
20. State that stale truth proves no match.
21. State that incomplete truth proves no match.
22. State that missing position means flat.
23. State that empty orders mean broker rejection.
24. Let a position alone prove this request filled.
25. Let a trade alone select an order.
26. Select the first of multiple candidates.
27. Select the latest of multiple candidates.
28. Select by broker status priority.
29. Fall back from FINAM venue symbol to broker-neutral symbol.
30. Allow same-request retry.
31. Allow automatic resend after ambiguity.
32. Treat an HTTP response as broker truth.
33. Make the historical cancel reconciler authoritative.
34. Make the M3d2 lifecycle authoritative.
35. Add real FINAM POST.
36. Add real FINAM DELETE.
37. Add reqwest order transport.
38. Add a Redis live command consumer.
39. Add broker dispatch.
40. Add runtime-live.
41. Add real strategy orders.
42. Add STOP/SLTP/bracket/replace/multi-leg behavior.
43. Open Stage 8B.
44. Permit shape matching before exact ClientOrderId.
45. Permit known BrokerOrderId after shape matching.
46. Convert unknown broker status to terminal.
47. Permit caller-selected or unbounded freshness/event policy.
48. Expose raw broker-truth identities or bodies in diagnostics.
49. Mark a saturated trade interval (`returned_count >= limit`) complete.
50. Admit trade history that misses the sealed event-window start.
51. Admit trade history that misses the sealed event-window end.
52. Allow caller-selected trade-history limit or recursive window.
53. Allow unbounded trade-interval subdivision.
54. Interpret exact GET-order 404 as `ProvenNoMatch`.
55. Let exact GET-order replace the account-wide orders safety snapshot.
56. Count an identical duplicate `BrokerTradeId` twice.
57. Silently deduplicate conflicting rows with one `BrokerTradeId`.
58. Lose filled quantity from cancelled-after-partial-fill state.
59. Lose filled quantity from expired-after-partial-fill state.
60. Accept rejected order with non-zero fill as exact.
61. Accept filled status when `filled_qty < qty`.
62. Accept inconsistent `remaining_qty`.
63. Let tier 3 ignore `OrderType`.
64. Let a MARKET request match a LIMIT candidate.
65. Let different LIMIT prices match.
66. Let contradictory TIF match.
67. Permit caller-selected or floating price tolerance.
68. Treat a missing required tier-3 shape field as a match.
