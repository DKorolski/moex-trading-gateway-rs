# Stage 8B-P R2A2 negative inventory

The automated source-bound harness rejects 26 contract mutations:

1. change the eleven-source inventory;
2. admit broker truth before network;
3. stop checking accepted fixed R1B identities;
4. stop checking authenticated dynamic claims;
5. disable constant-time account verification;
6. omit endpoint recomputation;
7. accept unknown/missing broker response shape;
8. omit position-baseline comparison;
9. remove the AuthService body cap;
10. widen the trades body cap;
11. accept a wrong CA;
12. accept a wrong hostname;
13. treat helper self-hash as authority;
14. open the R2A2 binary network entry;
15. issue authorization;
16. drift the local-receipt domain;
17. drift the account-HMAC domain;
18. drift the endpoint-identity domain;
19. widen the run lifetime;
20. remove a strict DTO/receipt schema guard;
21. remove a constant-time verifier;
22. remove the position comparison;
23. allow a non-working CANCEL target;
24. remove bounded streamed reads;
25. export raw response-body hashes;
26. reintroduce credential input into the R2A2 binary.

Rust adversarial tests additionally reject self-consistent unauthorized
manifests, forged and authentically stale receipts, wrong raw account/key
binding, wrong TokenDetails account, `{}` response shape, position mismatch,
wrong broker account/order ID, unknown status, terminal CANCEL target and
oversize bodies. Controlled TLS tests reject wrong trust root and hostname
before an HTTP request is observed.
