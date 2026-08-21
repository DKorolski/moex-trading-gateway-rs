# Stage 8B design R1 negative inventory

The exact mandatory mutation set contains 36 cases:

1. accepted Stage 8A-5 ref drift;
2. accepted review SHA-256 drift;
3. design-base ref drift;
4. reduce acceptance matrix below 48 rows;
5. reduce negative inventory below 36 cases;
6. open implementation directly after design acceptance;
7. remove a separately reviewed Stage 8B phase;
8. allow more than one command;
9. allow both PLACE and CANCEL in one run contract;
10. allow an automatic follow-up or LimitCancel pair;
11. allow an unbound/raw live account scope;
12. change the canonical instrument or venue symbol;
13. allow MARKET or quantity above one lot;
14. allow Stop/SLTP/bracket/replace/multi-leg;
15. make operator arm reusable or non-expiring;
16. omit build/config/endpoint/body binding from the arm;
17. reconstruct or mint a second arm after restart;
18. let caller snapshots authorize preflight;
19. omit fresh broker truth/readiness/schedule;
20. ignore kill-switch or ownership state;
21. allow ambiguity or unresolved lifecycle;
22. enter transport before durable attempt/fsync/covering seal;
23. treat Redis as execution authority;
24. retry automatically after the transport boundary;
25. classify timeout or malformed 2xx as definitely not sent;
26. resend after outcome unknown;
27. let current broker truth rewrite durable identity;
28. infer no order/flat from empty stale or account-wide counts;
29. permit conflict or still unknown to continue;
30. omit target-order/position/account/signoff post-run closure;
31. allow ALOR and FINAM simultaneous execution ownership;
32. allow redirects/proxies/alternate hosts/arbitrary routes;
33. leak secrets, raw authorization headers or live account ID;
34. omit no-send rehearsal/crash matrix/GET-only preflight evidence;
35. open any FINAM/Redis/publication/dispatch/runtime-live surface;
36. authorize a real request without separate `8B-X` acceptance and arm.
