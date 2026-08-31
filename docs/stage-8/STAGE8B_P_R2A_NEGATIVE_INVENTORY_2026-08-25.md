# Stage 8B-P R2A negative inventory

The R2A checker must reject all 40 mutations:

1. accepted merge ref changed;
2. accepted R1B ref changed;
3. R1B authority digest changed;
4. R1B network digest changed;
5. R1B run digest changed;
6. accepted build identity changed;
7. executable digest changed;
8. rebuild enabled;
9. alternate executable enabled;
10. operator selection marked present in R2A;
11. operation enum expanded;
12. instrument changed;
13. PLACE changed to MARKET;
14. quantity changed;
15. CANCEL exact-order requirement removed;
16. CANCEL lifecycle requirement removed;
17. token permitted in selection;
18. raw account permitted in evidence;
19. POST added to method allowlist;
20. source order changed;
21. route template changed;
22. max requests increased;
23. timeout changed;
24. minimum interval reduced;
25. preflight age widened;
26. retry enabled;
27. redirect enabled;
28. proxy enabled;
29. background loop enabled;
30. raw response export enabled;
31. required current input removed;
32. caller-built/cached truth allowed;
33. R2 evidence equated to K2 sources;
34. R2 permitted to satisfy K1;
35. R2 permitted to satisfy K2;
36. arm issuance enabled;
37. dispatch attempt enabled;
38. effect transport enabled;
39. R2B marked unlocked or GET already sent;
40. authorization changed from `NOT_ISSUED`.
