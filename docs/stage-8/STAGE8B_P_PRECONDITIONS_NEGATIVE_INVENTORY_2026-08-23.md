# Stage 8B-P preconditions R4 governance-closure negative inventory

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
19. Disable the active ruleset.
20. Remove the default-main target.
21. Remove the branch-deletion rule.
22. Remove the non-fast-forward/force-push rule.
23. Remove the pull-request rule.
24. Restore a nonzero GitHub approval count and recreate the solo-owner deadlock.
25. Restore stale-review approval handling in the zero-approval policy.
26. Restore approval of the last push in the zero-approval policy.
27. Disable review-thread resolution.
28. Restore extra approval for unattributed changes in the zero-approval policy.
29. Permit squash merging in addition to the frozen merge method.
30. Remove the `redis-smoke` required check.
31. Remove the `rust` required check.
32. Disable strict required-check policy.
33. Add a ruleset bypass actor.
34. Claim the main branch is not protected.
35. Delete `active_main_ruleset_required`.
36. Delete `pull_request_required`.
37. Delete `zero_github_approvals_required_for_solo_mode`.
38. Delete `canonical_status_checks_required`.
39. Delete `force_push_blocked_required`.
40. Delete `branch_deletion_blocked_required`.
41. Delete `empty_bypass_policy_required`.
42. Delete `immutable_post_merge_closure_evidence_required`.
43. Delete `current_tree_gate_required`.
44. Delete `independent_engineering_acceptance_required_for_stage8b_p`.
45. Add an unreviewed governance policy key.
46. Revert checkout to a mutable tag.
47. Change the checkout action full SHA.
48. Revert the Rust action to a mutable tag.
49. Change the Rust action full SHA.
50. Revert the Rust toolchain to `stable`.
51. Replace the exact accepted solo-mode status with generic in-band acceptance.
52. Open Stage 8B-P or an effect surface.
53. Change the next action from reviewed merge verification to execution.
54. Remove the explicit operator solo-mode authorization.
55. Change the solo-mode approval count away from zero.
56. Remove independent engineering review as a Stage 8B-P prerequisite.
57. Promote a GitHub approval to semantic acceptance evidence.
58. Change the reviewed R3 candidate ref in immutable merge evidence.
59. Change the normal merge commit ref in immutable merge evidence.
60. Change the verified post-merge tree.
61. Mark the candidate and merge trees non-identical.
62. Change a required candidate check from success.
63. Remove the live GitHub API verification claim from merge closure.
64. Rebind candidate checks to a different commit.
