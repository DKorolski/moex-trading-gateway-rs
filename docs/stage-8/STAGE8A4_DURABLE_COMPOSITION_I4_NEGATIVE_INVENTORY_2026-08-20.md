# Stage 8A-4 I4 implementation negative inventory

The implementation gate mutates and rejects at least these boundaries:

1. terminal authority becomes caller-constructible;
2. terminal authority becomes Clone;
3. owner issuer stops rereading S1;
4. owner issuer calls seal advancement;
5. complete suffix requirement is removed;
6. exact-transition requirement is removed;
7. CANCEL ExactWorking becomes terminal;
8. ACK identity changes to a new domain;
9. ACK facts gain a timestamp;
10. current readiness stops requiring RunAllowed;
11. account active-order zero guard is removed;
12. target active-order zero guard is removed;
13. current source evidence stops binding accepted config;
14. I4 facade becomes public/exported;
15. Redis/FINAM/publication or execution capability is attached.
16. raw snapshot DTOs replace `Stage8a1TrustedCurrentSources` at the final mint;
17. caller-supplied `now` controls freshness;
18. `max_orders != 1` remains Ready;
19. `consumed_orders != 0` remains Ready;
20. deterministic strategy-instance mapping is removed;
21. trusted-current-source root/evidence validation is removed;
22. current-control revision/tamper validation is removed;
23. checkpoint getter becomes public downstream;
24. seal generation or commitment getter becomes public downstream;
25. settlement-authority fingerprint getter becomes public downstream;
26. inherited 64-row Design R3 traceability is reduced;
27. inherited 46-case Design R3 negative gate is skipped.
28. I4 composer again requires the pre-finalization operational issuer;
29. a finalized request is forced through dispatch-ready Stage8A1 authority;
30. the I4-only read-only terminal issuer is removed;
31. the fresh-process post-S1 I4 witness is removed;
32. process B retains a process-A issuer/capability;
33. readiness-unavailable restart drops or changes historical ACK;
34. restart-critical trace rows lose their exact concrete witness.
