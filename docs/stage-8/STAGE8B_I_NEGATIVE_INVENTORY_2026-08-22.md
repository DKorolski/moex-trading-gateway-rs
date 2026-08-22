# Stage 8B-I negative inventory

Every mutation must be rejected by `stage8b_i_check.py --root <mutated-tree>`.

1. accepted S-R3 candidate drifts;
2. accepted S-R3 merge drifts;
3. accepted S-R3 tree drifts;
4. accepted S authority digest drifts;
5. public facade uniqueness is weakened;
6. private root uniqueness is weakened;
7. broker-cli positive integration is removed;
8. external private-root compile failure is removed;
9. authority privacy is weakened;
10. authority trait prohibition is weakened;
11. accepted Stage8A2 source digest drifts;
12. existing-builder-only ownership is weakened;
13. accepted Stage8A3 source digest drifts;
14. Model A ownership is weakened;
15. HMAC algorithm/domain rule is weakened;
16. HMAC suffix changes;
17. HMAC length encoding changes;
18. minimum HMAC key is reduced;
19. golden HMAC digest drifts;
20. constant-time verification is removed;
21. secret zeroization is removed;
22. absolute-path requirement is removed;
23. symlink rejection is removed;
24. single-link requirement is removed;
25. O_NOFOLLOW is removed;
26. descriptor/path identity recheck is removed;
27. path-swap negative is removed;
28. manifest openat ownership is removed;
29. bounded reads are removed;
30. durable arm O_EXCL is removed;
31. arm fsync requirement is removed;
32. cross-process single-winner proof is removed;
33. kill-boundary count is reduced;
34. crash-window count is reduced;
35. durable restart prefix count is reduced;
36. impossible replay sequence becomes accepted;
37. closure class count is reduced;
38. automatic retry/resend is opened;
39. any execution/live surface is opened;
40. acceptance or negative count drifts.
41. K2 root invokes Stage 8A-2 builder before permit;
42. builder bridge accepts a capability instead of consuming exact permit;
43. fresh preflight stores request parts;
44. sealed-attempt transition is removed;
45. exact permit transition is removed or permit becomes reusable;
46. private request witness or raw parts escape;
47. durable request argument is ignored or its consumed binding is removed;
48. K2 fresh-source witness is omitted;
49. build input is removed from K2 binding;
50. keyed account input is removed from K2 binding;
51. contract/config/policy/endpoint input is removed from K2 binding;
52. run/body input is removed from K2 binding;
53. K1 control input is removed from K2 binding;
54. exact arm input is removed from K2 binding;
55. single FINAM owner predicate is weakened;
56. zero ambiguity predicate is weakened;
57. zero unresolved-lifecycle predicate is weakened;
58. readiness schedule or broker-truth freshness predicate is weakened;
59. durable closure payload is removed;
60. residual unknown or conflict is normalized to closed-safe;
61. torn unknown mismatched or corrupt closure payload is accepted;
62. build verifier permits a legacy-send feature or unknown feature state;
63. endpoint identity omits method template account binding or renderer;
64. K2 authentication/record-digest/fresh-timestamp/expiry validation is bypassed.
65. K2 accepts zero or more than one remaining invocation budget.
66. K3 covering seal is omitted from the durable attempt binding.
67. K4 does not recheck the exact sealed attempt identity.
68. K5 reconciliation transition is removed or bypassed.
69. K5 control authority is omitted from the durable closure binding.
70. exact durable closure publication transition is removed or normalized.
