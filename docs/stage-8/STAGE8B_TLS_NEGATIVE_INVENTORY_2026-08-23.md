# Stage 8B-IT-TLS negative inventory

The R1 gate must reject all 40 mutations:

1. Change the accepted predecessor ref.
2. Change the accepted IT-R3 adapter hash.
3. Change the TLS successor adapter hash.
4. Change the TLS harness hash.
5. Change Cargo.lock hash.
6. Change the production graph hash.
7. Change the qualification graph hash.
8. Open Stage 8B-P.
9. Open Stage 8B-XE.
10. Open real FINAM POST/DELETE.
11. Open broker effect.
12. Open Redis execution consumer.
13. Replace `retry::never()`.
14. Remove redirect denial.
15. Remove `no_proxy()`.
16. Change the connect timeout.
17. Change the request timeout.
18. Re-enable idle connection reuse.
19. Enable built-in roots for qualification.
20. Remove the explicit local root.
21. Remove fixed-host loopback resolution.
22. Change the controlled hostname to the FINAM hostname.
23. Change the reserved `.invalid` hostname.
24. Allow non-loopback resolution.
25. Add `danger_accept_invalid_certs`.
26. Add `danger_accept_invalid_hostnames`.
27. Add native-tls.
28. Enable reqwest default features.
29. Change the rustls provider from ring.
30. Bind the controlled server to all interfaces.
31. Remove ALPN h2.
32. Remove the valid PLACE HTTP2 test.
33. Remove the valid CANCEL HTTP2 test.
34. Remove wrong-CA coverage.
35. Remove wrong-hostname coverage.
36. Remove expired-certificate coverage.
37. Remove not-yet-valid coverage.
38. Remove TLS timeout coverage.
39. Remove TLS response-loss coverage.
40. Add a production TLS endpoint constructor.
