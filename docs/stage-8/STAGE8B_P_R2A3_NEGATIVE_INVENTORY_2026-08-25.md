# Stage 8B-P R2A3 negative inventory

The R2A3 harness retains the R2A2 semantic categories and adds focused
mutations for the review findings. It must reject changes that:

1. issue authorization or include a real credential;
2. remove a current official document or change its hash;
3. remove strict DTO/golden coverage or `triggered_order_id`;
4. remove the runnable one-shot entry or make paths caller-selected;
5. forward ambient environment or replace fd-bound execution;
6. merge producer, issuer and verifier custody;
7. permit arbitrary claim names, zero generation or nonce mismatch;
8. remove signature source binding or durable nonce replay protection;
9. widen either skew limit or the 250 ms pacing interval;
10. omit pre-class, pre-GET or final freshness revalidation;
11. stop parsing token timestamps or exact readonly account scope;
12. allow PLACE after a matching trade;
13. allow CANCEL trade/order mismatch;
14. weaken exact/list immutable equality;
15. add real order POST/DELETE, Redis execution or runtime-live.

The inherited R1/R1A/R1B gates remain separate mandatory predecessors. R2A3
does not reinterpret their accepted authority.
