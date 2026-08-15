# Stage 8A-2 R1 semantic negative inventory — exact 37 cases

1. accept an unrevalidated `Stage8ExecutionCapability`
2. accept a diagnostic instead of the continuation
3. reconstruct a capability
4. reconstruct an approved command
5. expose an approved-command getter
6. borrow rather than consume the continuation
7. clone or reuse continuation authority
8. hand-build a PLACE DTO
9. hand-build a CANCEL request/path
10. add a second PLACE/CANCEL serializer
11. bypass the accepted PLACE builder
12. bypass the accepted CANCEL builder
13. change PLACE outgoing comment from `None` to `Some(...)`
14. export `FinamPlaceOrderRequestSpec`
15. export `FinamCancelOrderRequestSpec`
16. export raw body or serialized JSON
17. export raw path or path segments
18. export raw account/order/client identifiers
19. export a transport-ready URL/request builder
20. import `M3d2RealOrderEndpointTransport`
21. invoke historical PLACE execution
22. invoke historical CANCEL execution
23. import `EndpointGateApproved`
24. add `reqwest` to the Stage 8A-2 path
25. add `.post(`
26. add `.delete(`
27. add `.send(`
28. reach or enable `m3j16-actual-one-shot`
29. add an external FINAM base URL or endpoint
30. add HTTP outcome classification
31. add automatic retry
32. construct `ProvenNoMatch`
33. invoke broker reconciliation
34. attach Redis live command consumption
35. attach broker dispatch or runtime-live
36. issue real strategy orders
37. add STOP/SLTP/bracket/replace/multi-leg behavior
