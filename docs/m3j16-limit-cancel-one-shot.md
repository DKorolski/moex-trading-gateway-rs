# M3j-16 LimitCancel one-shot package

M3j-16 is a reviewed, operator-triggered FINAM boundary package for a single
below-market buy limit order followed by cancellation. It is not a runtime live
attachment and it does not connect the command consumer to real FINAM.

Default mode is no-send dry-run:

```bash
cargo run -p broker-cli -- finam-limit-cancel-one-shot \
  --limit-price 2210 \
  --reference-price 2223 \
  --qty 1 \
  --output reports/m3j16-limit-cancel-one-shot/redacted-dry-run-report.json
```

The actual boundary call is intentionally behind two controls:

- the binary must be built with `--features m3j16-actual-one-shot`;
- the operator must pass `--actual-send-i-understand-risk`.

For the first approved invocation the intended operator scope is:

```text
symbol      = IMOEXF@RTSX
side        = buy
order_type  = limit
limit_price = 2210
qty         = 1
max_orders  = 1
flow        = place then cancel
```

Hard boundaries:

- no market entry;
- no stop / SLTP / bracket;
- no replace;
- no multi-leg;
- no runtime continuous live attachment;
- no command-consumer-to-real-FINAM;
- no raw curl / out-of-band POST.

Before actual send the report must show:

```text
symbol_exact_match_or_hash = true
account_operator_binding_ok = true
reference_quote_bound_to_fresh_artifact = true
positions_count = 0
active_orders_count = 0
unknown_active_orders_count = 0
orphan_active_orders_count = 0
broker_truth_clean = true
limit_price_below_reference = true
```

M3j-16a final gate can be proven without sending by building with the feature
and passing the actual flag plus `--pre-actual-gate-only`:

```bash
cargo run -p broker-cli --features m3j16-actual-one-shot -- \
  finam-limit-cancel-one-shot \
  --limit-price 2210 \
  --reference-price 2223 \
  --qty 1 \
  --actual-send-i-understand-risk \
  --pre-actual-gate-only \
  --output reports/m3j16-limit-cancel-one-shot/redacted-pre-actual-gate-report.json
```

That mode may show `actual_send_allowed = true`, but still records
`boundary_invocation_performed = false`.

If place succeeds but FINAM does not return a broker order id, the package does
not guess a cancel id. It records `broker_order_id_present = false`,
`cancel_attempted = false`, and requires post-run reconciliation/manual review.
