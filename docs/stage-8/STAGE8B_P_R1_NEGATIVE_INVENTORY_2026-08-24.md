# Stage 8B-P R1 negative inventory

The R1 checker and negative harness must reject every mutation below.

1. Change the accepted main predecessor ref.
2. Change the accepted predecessor tree.
3. Remove the accepted GOV-P1 status.
4. Change the preconditions authority SHA-256.
5. Change the governance observation SHA-256.
6. Change the accepted TLS source ref.
7. Change the accepted source archive SHA-256.
8. Change the accepted executable SHA-256.
9. Change the accepted target triple.
10. Change the accepted Rust release.
11. Enable the broker-cli legacy actual-send feature.
12. Enable the finam-gateway legacy actual-send feature.
13. Claim production-code drift after TLS qualification.
14. Change the fresh contract snapshot SHA-256.
15. Change the official response count.
16. Mark any fresh official response non-200.
17. Mark fresh hashes different from the accepted contract.
18. Claim material FINAM contract drift.
19. Claim that credentials were used for documentation refresh.
20. Claim a broker read-only GET was sent.
21. Claim a FINAM order request was sent.
22. Remove a required exact-run manifest field.
23. Add an unreviewed exact-run manifest field.
24. Allow more than one operation in a run.
25. Add an operation other than PLACE or CANCEL.
26. Change the PLACE instrument.
27. Change PLACE from LIMIT.
28. Change PLACE from DAY.
29. Raise PLACE quantity above one.
30. Permit CANCEL without exact working same-lifecycle target.
31. Permit MARKET.
32. Permit Stop SLTP bracket replace or multi-leg.
33. Permit automatic transport retry.
34. Permit same-request resend.
35. Permit a LimitCancel pair in one run.
36. Permit caller-built or cached broker truth.
37. Permit GET-only preflight to issue an operator arm.
38. Permit GET-only preflight to record a dispatch attempt.
39. Permit GET-only preflight to enter transport.
40. Claim this package issued an operator arm.
41. Make the arm constructible by this package.
42. Remove one-shot arm semantics.
43. Permit Clone Copy or serialization of the arm.
44. Permit arm reconstruction.
45. Change authorization status from `NOT_ISSUED`.
46. Open Stage 8B-P.
47. Open the broker-effect surface.
48. Skip R2 and make Stage 8B-XE the next action.
