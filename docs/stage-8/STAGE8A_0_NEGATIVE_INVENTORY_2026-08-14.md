# Stage 8A-0 negative inventory — exact 36 cases

1. `forge accepted Gate R3 ref or review SHA`
2. `add crates/** production Rust change`
3. `modify Cargo.toml/Cargo.lock`
4. `modify .github workflow`
5. `open 8A-1/8A-2 directly from Gate`
6. `enable FINAM POST/DELETE`
7. `omit official REST source URL/timestamp`
8. `omit official enum/protobuf source`
9. `change normalized snapshot without SHA`
10. `hand-edit non-reproducible snapshot`
11. `change PLACE endpoint path`
12. `change CANCEL endpoint path`
13. `omit documented PLACE field`
14. `drop a current OrderType enum value`
15. `drop a current TimeInForce enum value`
16. `allow non-DAY TIF in Stage8 initial scope`
17. `omit ClientOrderId max-20 broker constraint`
18. `allow broker-generated/omitted ClientOrderId`
19. `enable arbitrary outgoing comment`
20. `drop PLACE status/default`
21. `drop CANCEL 401`
22. `CANCEL 400 -> BrokerRejected`
23. `CANCEL 401 -> ordinary reject or retry`
24. `CANCEL 404 -> success/rejected terminal`
25. `CANCEL 409/410 -> success`
26. `introduce second PLACE/CANCEL serializer`
27. `silently accept official-vs-builder drift`
28. `ignore new/unknown OrderStatus`
29. `omit instrument/schedule prerequisite source`
30. `DefinitelyNotSent -> same durable request retry`
31. `add Stage8 journal/reducer/identity allocator`
32. `reuse Stage5 forbidden scanner as sole Stage8 authority`
33. `auto-fix production mapper inside 8A-0`
34. `unpin negative count`
35. `skip workspace regression`
36. `self-accept 8A-0 or open 8A-2`
