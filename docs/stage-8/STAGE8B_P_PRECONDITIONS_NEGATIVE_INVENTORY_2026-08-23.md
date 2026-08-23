# Stage 8B-P preconditions refresh negative inventory

The checker and negative harness must reject every mutation below.

1. Change the accepted TLS source commit.
2. Change the accepted TLS tree.
3. Change the accepted TLS archive SHA-256.
4. Mark the accepted TLS merge as non-identical.
5. Remove an official FINAM response.
6. Change an official FINAM response status from 200.
7. Change an official FINAM response SHA-256.
8. Claim material contract drift.
9. Change the production host.
10. Change PLACE method or route.
11. Change CANCEL method or route.
12. Enable automatic retry or same-request resend.
13. Change the accepted source archive used by the build.
14. Change the reproducible executable SHA-256.
15. Reduce reproducible clean build count below two.
16. Mark independent build hashes non-identical.
17. Change Cargo.lock identity.
18. Change rustc commit identity.
19. Claim branch protection is active.
20. Claim the disabled ruleset is active.
21. Remove a mutable CI reference observation.
22. Mark GOV-P1 accepted in-band.
23. Open Stage 8B-P or any closed effect surface.
24. Change the next action from independent review to execution.
