# Stage 8A-1 negative inventory

The mandatory harness contains exactly 36 mutations:

1. self-accept Stage 8A-1;
2. forge the accepted Stage 8A-0 predecessor;
3. forge the external acceptance hash;
4. open Stage 8A-2;
5. authorize FINAM POST/DELETE;
6. derive `Clone` for the capability;
7. derive `Debug` for the capability;
8. derive `Serialize` for the capability;
9. expose a public capability field;
10. add a request-extraction API;
11. invoke the PLACE builder;
12. invoke the CANCEL builder;
13. add a `reqwest` transport dependency in the module;
14. add a `.send(` call;
15. add a `.post(` call;
16. add Redis command access;
17. remove the one-shot arm check;
18. remove account allowlist enforcement;
19. remove instrument allowlist enforcement;
20. remove strategy allowlist enforcement;
21. remove DAY-only enforcement;
22. remove the `RunAllowed` requirement;
23. remove durable kill-switch revision enforcement;
24. remove exact FINAM ownership;
25. remove the single-owner count;
26. remove zero unresolved-order enforcement;
27. remove zero unknown-delivery enforcement;
28. remove zero reconciliation-required enforcement;
29. remove restart-generation binding;
30. remove configuration-fingerprint binding;
31. remove exact CANCEL durable mapping;
32. permit cancel without mapping;
33. remove Clone compile-fail evidence;
34. remove Serialize compile-fail evidence;
35. reduce the acceptance matrix;
36. remove the accepted predecessor freeze hash.
