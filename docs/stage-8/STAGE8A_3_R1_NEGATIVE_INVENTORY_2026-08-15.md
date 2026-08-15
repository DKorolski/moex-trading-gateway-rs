# Stage 8A-3 R1 semantic negative inventory

The final gate must reject exactly these 42 mutation classes. The count may
increase in a later reviewed revision but must not shrink silently.

1. Add a contextless classifier defaulting to PLACE.
2. Call the historical contextless classifier.
3. Call the historical context-aware classifier as authority.
4. Add a generic 400..500 to broker-rejected policy.
5. Classify PLACE 404 as broker rejected.
6. Classify CANCEL 400 as broker rejected.
7. Classify CANCEL 404 as broker rejected.
8. Classify CANCEL 409 as broker rejected.
9. Classify CANCEL 410 as broker rejected.
10. Convert 429 into retry authority.
11. Convert Retry-After into reusable execution authority.
12. Convert 500 into retry or maintenance terminal authority.
13. Convert 503 into retry or maintenance terminal authority.
14. Convert 504 into retry or definitely-not-sent authority.
15. Convert timeout into definitely-not-sent.
16. Convert body-read failure into definitely-not-sent.
17. Convert a connect-error category alone into definitely-not-sent.
18. Accept an undocumented PLACE 2xx.
19. Accept PLACE 200 without broker order identity.
20. Accept PLACE 200 with empty broker order identity.
21. Accept PLACE 200 with correlation mismatch.
22. Classify PLACE 400 as rejected from status alone.
23. Classify PLACE 400 as rejected through free-text matching.
24. Accept CANCEL 204.
25. Accept an empty CANCEL body regardless of status.
26. Convert CANCEL 200 into flat, terminal or no-fill truth.
27. Convert CANCEL 401 into ordinary rejection.
28. Allow same-request retry after CANCEL 401.
29. Allow same-request retry after any ambiguous observation.
30. Expose the raw response body.
31. Expose the raw broker order identity.
32. Expose raw ClientOrderId or account identity.
33. Construct ProvenNoMatch.
34. Call Stage 8A-4 reconciliation.
35. Import or use a real reqwest send surface.
36. Call M3d2 real transport.
37. Use EndpointGateApproved.
38. Enable m3j16 actual one-shot execution.
39. Attach a Redis live consumer.
40. Attach broker dispatch or runtime-live.
41. Issue a real strategy order.
42. Add STOP, SLTP, bracket, replace or multi-leg behavior.
