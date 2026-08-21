# Stage 8A-5 aggregate acceptance negative inventory

The aggregate checker and mutation harness must reject at least:

1. accepted I4 predecessor drift;
2. accepted Stage7B ref drift;
3. accepted Stage8A-0 ref drift;
4. accepted Stage8A-1 ref drift;
5. accepted Stage8A-2 ref drift;
6. accepted Stage8A-3 ref drift;
7. accepted Stage8A-4 reducer ref drift;
8. accepted durable-design ref drift;
9. accepted implementation-spec ref drift;
10. accepted I1/I2/I3/I4 lineage drift;
11. aggregate matrix row reduction;
12. production Rust change authorization;
13. Cargo/lock change authorization;
14. workflow change authorization;
15. inherited Stage7B gate omission;
16. inherited Stage8 semantic/negative rerun omission;
17. Stage8 forbidden-surface scan omission;
18. debug or release workspace test omission;
19. external compile-boundary omission;
20. ACK/Redis/FINAM/dispatch/runtime-live/real-order or Stage8B opening.
