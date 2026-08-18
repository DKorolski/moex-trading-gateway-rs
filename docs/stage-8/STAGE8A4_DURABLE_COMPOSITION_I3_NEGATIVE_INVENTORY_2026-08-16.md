# Stage 8A-4 I3 R2 negative inventory

The fail-closed mutation harness pins these 48 independent cases:

1. accepted I2 predecessor drift;
2. accepted I2 review hash drift;
3. rejected I3 R1 reference drift;
4. rejected I3 R1 review hash drift;
5. premature I3 R2 acceptance;
6. wrong branch authority;
7. raw batch writer marked callable;
8. raw core append marked public;
9. sealed linear authority disabled;
10. exact request truth binding disabled;
11. writer-entry freshness disabled;
12. exact control binding disabled;
13. post-write sticky poison disabled;
14. suffix fault matrix disabled;
15. ACK/readiness opened;
16. Redis live opened;
17. FINAM POST/DELETE opened;
18. runtime-live opened;
19. Stage8A5 opened;
20. public raw Stage7 batch writer restored;
21. sealed Stage7 authority entry removed;
22. raw core Stage8A4 append restored as public;
23. raw core append re-exported;
24. private I3 issuer exported;
25. opaque authority definition removed;
26. external raw-writer compile-fail proof removed;
27. opaque-authority compile-fail proof removed;
28. exact truth request binding removed;
29. exact truth account binding removed;
30. exact truth instrument binding removed;
31. exact truth durable binding removed;
32. writer-entry truth freshness removed;
33. control operational identity binding removed;
34. control runtime-config binding removed;
35. control scope binding removed;
36. arm-registry binding removed;
37. mutation uncertainty classification removed;
38. owner sticky mutation flag removed;
39. owner poison assignment removed;
40. poisoned recovery-ready guard removed;
41. poisoned owner seal-advance assertion removed;
42. V2 post-write fault test removed;
43. suffix post-write fault test removed;
44. pre-write non-poison test removed;
45. narrow V2-only restart test removed;
46. unrelated suffix rejection test removed;
47. raw transport marker added to private I3 module;
48. acceptance matrix row removed.

Inherited I2, forbidden-surface, compiler, clippy and archive-safety gates remain
mandatory. In particular, the checker rejects both a raw Stage7 writer and a
normal public core append even if all documentation still claims closure.
