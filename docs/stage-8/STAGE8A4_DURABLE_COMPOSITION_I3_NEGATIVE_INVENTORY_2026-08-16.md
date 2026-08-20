# Stage 8A-4 I3 R5 negative inventory

The fail-closed mutation harness retains all 80 R4 protections and pins 88
independent cases:

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
48. acceptance matrix row removed;
49. renamed public raw core append;
50. `#[doc(hidden)] pub` raw core append;
51. external crate raw-mutator compile-fail proof removed;
52. runtime-durable-service depends on finam-gateway;
53. broker-specific dependency added to broker-neutral runtime;
54. public Stage8A1 `from_current_stage6_authority` restored;
55. caller-provided Stage8A1 seal/op-id accepted;
56. local Stage8A1 readiness lookalike replaces Stage7B readiness;
57. production sealed normal composition removed;
58. crash suffix recovery requires a test helper or lost I2 candidate.
59. rejected I3 R3 reference drifts;
60. rejected I3 R3 review digest drifts;
61. public sealed-authority construction is enabled;
62. typed Pending restart is disabled;
63. source/truth/control authority binding is disabled;
64. core-private sealed authority becomes public;
65. Pending owner is removed;
66. Pending restart outcome is converted to ordinary Ready;
67. production source bridge is removed;
68. production durable-context issuer is removed;
69. frozen production policy issuer is removed;
70. read-only source-evidence issuer is removed;
71. Ed25519 writer-attestation verification is removed;
72. authenticated operational issuer-key pin comparison is removed;
73. private signing-key permission check is removed;
74. caller-forgeable validated-entry issuer is restored;
75. production normal-path integration test is removed;
76. production V2-only recovery integration test is removed;
77. production partial-suffix recovery integration test is removed;
78. production complete-batch-before-S1 integration test is removed;
79. production recovery test is redirected to a raw/test batch writer;
80. complete uncovered batch is excluded from Pending recovery material.
81. recovery function again requires `Stage8ExecutionCapability`;
82. recovery function again requires a retained `Stage8a1OperationalAuthorityIssuer`;
83. recovery-only issuer construction from Pending owner is removed;
84. deterministic historical arm nonce reconstruction is removed;
85. recovery recreates an arm through `register_arm_nonce`;
86. historical arm-registration signature verification is removed;
87. same-process recovery retains the pre-crash capability or issuer;
88. the SIGKILL fresh-process recovery witness is removed.

Inherited I2, forbidden-surface, compiler, clippy and archive-safety gates remain
mandatory. In particular, the checker rejects both a raw Stage7 writer and a
normal public core append even if all documentation still claims closure.
