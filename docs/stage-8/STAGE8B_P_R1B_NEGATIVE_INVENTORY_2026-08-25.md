# Stage 8B-P R1B negative inventory

R1B adds 36 mutations to the inherited R1/R1A 98/98 matrix:

1. R1A authority digest changed;
2. network endpoint authority digest changed;
3. accepted run authority digest changed;
4. qualified endpoint domain changed;
5. endpoint digest encoding changed;
6. operation inserted as an endpoint digest component;
7. NUL-delimited endpoint encoding enabled;
8. endpoint components reordered;
9. endpoint length width changed;
10. endpoint byte order changed;
11. PLACE method changed;
12. CANCEL route changed;
13. golden account binding changed;
14. golden renderer changed;
15. PLACE endpoint golden changed;
16. CANCEL endpoint golden changed;
17. run identity domain changed;
18. run identity encoding changed;
19. common run fields reordered;
20. PLACE run fields reordered;
21. CANCEL run fields reordered;
22. operation discriminator omitted;
23. run identity included in its own preimage;
24. computed-and-verified rule removed;
25. caller-asserted run digest allowed;
26. PLACE run golden replaced arbitrarily;
27. CANCEL run golden replaced arbitrarily;
28. endpoint changed while PLACE run digest retained;
29. body changed while PLACE run digest retained;
30. freshness authority changed while PLACE run digest retained;
31. execution build changed while PLACE run digest retained;
32. operation omitted from CANCEL manifest;
33. approved position made noncanonical;
34. generation made noncanonical;
35. expiry representation changed;
36. authorization changed from `NOT_ISSUED`.

All 36 must fail through the R1B checker. The combined gate separately replays
the inherited 48 R1 and 50 R1A mutations, for 134/134 total.
