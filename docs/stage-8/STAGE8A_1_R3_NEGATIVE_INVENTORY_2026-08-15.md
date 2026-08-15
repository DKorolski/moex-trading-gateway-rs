# Stage 8A-1 R3 negative inventory — exact 70 cases

1. `self-accept Stage8A-1 R1`
2. `forge accepted Stage8A-0 ref`
3. `forge Stage8A-0 review hash`
4. `open Stage8A-2 before acceptance`
5. `derive Clone/Copy on capability`
6. `derive Debug on capability`
7. `derive Serialize/Deserialize on capability`
8. `expose capability field`
9. `add request/approved-command extraction`
10. `invoke PLACE/CANCEL builder`
11. `add HTTP/send/Redis consumer surface`
12. `add Stage8 journal/reducer/ClientOrderId allocator`
13. `mint capability without Stage7B recovery-ready authority`
14. `bind stale/mismatched recovery seal`
15. `mint capability from raw PlaceOrder without Stage6 durable authority`
16. `change PLACE durable ClientOrderId for same request`
17. `change CANCEL durable command identity or target identity`
18. `change Stage6 strategy owner/cycle/action attribution`
19. `mint while composite readiness is absent/lost`
20. `construct safety proof using public struct literal`
21. `make operator arm fields public`
22. `make operator arm Clone/Serialize`
23. `reuse same operator arm authority twice`
24. `duplicate/replay arm nonce`
25. `change StrategyRequestId under same arm`
26. `change ClientOrderId under same arm`
27. `change account/instrument/venue under same arm`
28. `change strategy owner/cycle/action under same arm`
29. `change side under same arm`
30. `change quantity under same arm`
31. `change LIMIT price under same arm`
32. `change MARKET reference guard under same arm`
33. `widen max notional/slippage/reference-age under same arm`
34. `change build/config/endpoint-policy digest under same arm`
35. `allow caller-chosen unbounded arm TTL`
36. `reuse arm/capability after restart`
37. `reuse authority after config drift`
38. `construct arbitrary allowlist/frozen policy`
39. `allow unsupported order type or non-DAY TIF`
40. `widen max_qty/qty_step/price_step/max_market_qty policy`
41. `remove max_notional_per_order/run`
42. `disable slippage/reference-age bound`
43. `ignore closed/unknown trading schedule`
44. `forge RunAllowed kill-switch evidence from raw fields/hash-shaped string`
45. `use single-broker ownership evidence for another strategy`
46. `use zero-ambiguity evidence for another account/strategy`
47. `omit or stale accepted fresh broker truth`
48. `omit/exhaust max-one engineering-micro budget`
49. `allow caller-chosen huge evidence age or backdated now`
50. `mix restart/config/seal/scope across proof authorities`
51. `mint capability for terminal/unmapped CANCEL`
52. `inject hidden builder/transport hook into already-allowed lib.rs`
53. `delete committed recovery seal immediately before Stage8 durable authority issuance`
54. `corrupt or replace committed recovery seal while cached seal remains unchanged`
55. `mint Stage8 durable authority from RequestAccepted without durable DispatchAttemptRecorded`
56. `use only restart fixture as forward-path authority proof`
57. `leave non-durable Stage8 opaque authorities without production issuers`
58. `use private test literals as the only positive authority-construction path`
59. `accept duplicate logical operator-arm nonce in production issuer`
60. `forge max-one micro budget from an arbitrary snapshot instead of trusted issuer`
61. `consume previously minted capability after kill-switch/config/readiness state drift without revalidation`
62. `introduce FINAM builder/request extraction while performing R2 authority work`
63. `open a caller-created temporary authority root with self-consistent accepted config and sidecar`
64. `widen broker policy by rewriting accepted config and its adjacent SHA sidecar before issuer open`
65. `forge RunAllowed owner-count and free-budget state by writing current-control JSON in caller root`
66. `mint from caller-constructed readiness/broker-truth snapshots without accepted current-source authority`
67. `mint a second capability for the same durable request using a different logical arm nonce`
68. `allow public caller to choose a fresh arm uniqueness token instead of trusted operator issuer`
69. `mint CANCEL capability then change kill-switch/config/readiness/truth and bypass current-state revalidation`
70. `swap/replace/symlink the file-backed issuer root or current-control path after open`
