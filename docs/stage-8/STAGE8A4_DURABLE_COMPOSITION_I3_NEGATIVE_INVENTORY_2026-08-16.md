# Stage 8A-4 I3 negative inventory

The I3 checker and mutation harness pin at least these fail-closed mutations:

1. accepted I2 predecessor drift;
2. accepted review hash drift;
3. premature I3 acceptance;
4. ACK/readiness opened;
5. Redis live opened;
6. FINAM POST/DELETE opened;
7. runtime-live opened;
8. Stage8A5 opened;
9. sole writer entry removed;
10. S0 reread removed;
11. current Stage6 authority reconstruction removed;
12. durable binding recomputation removed;
13. frontier CAS removed;
14. seal-generation CAS removed;
15. seal-fingerprint CAS removed;
16. request-state CAS removed;
17. previous-record comparison removed;
18. lifecycle-sequence comparison removed;
19. global-tail comparison removed;
20. same-key/different-payload conflict removed;
21. V2-first append removed;
22. suffix-prefix verification removed;
23. complete-batch check removed;
24. covering S1 advancement removed;
25. covering S1 reread removed;
26. narrow journal-ahead V2 requirement removed;
27. second-V2 rejection removed;
28. arm-registry validation removed;
29. current control evidence binding removed;
30. CANCEL original durable shape validation removed;
31. immediate account-safety comparison removed;
32. stale frontier test removed;
33. stable-key collision test removed;
34. V2-only crash test removed;
35. partial-suffix crash test removed;
36. durable receipt converted to ACK/readiness authority;
37. raw transport marker added to private I3 module;
38. acceptance matrix row removed.

Inherited Stage 8A-4 I2 and project forbidden-surface gates remain mandatory.
