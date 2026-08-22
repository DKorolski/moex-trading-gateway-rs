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
