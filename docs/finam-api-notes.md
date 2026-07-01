# Finam API notes — initial read

Sources:

- https://api.finam.ru/getting-started/
- https://api.finam.ru/docs/rest/
- https://api.finam.ru/docs/grpc/
- https://api.finam.ru/docs/async-api/

## Primary observations

Finam Trade API supports REST, gRPC, and WebSocket.

Endpoints from the public docs:

- REST: `https://api.finam.ru`
- gRPC: `api.finam.ru:443`
- WebSocket: `api.finam.ru/ws` or `api.finam.ru/tradinginfo`

Auth model:

- User generates a secret token in the Finam token portal.
- The secret is exchanged for a short-lived JWT/access token.
- JWT is used in API requests.
- The public getting-started page says JWT lifetime is 15 minutes.
- gRPC docs recommend `SubscribeJwtRenewal` for automatic renewal.

Limits and operations:

- Public getting-started page states a limit of 200 requests per minute per method.
- Public docs mention daily service interval 05:00–06:15 Moscow time.
- Public FAQ notes stream disconnection once per day after 86400 seconds from subscription start.
- HTTP/2 is recommended; public FAQ says HTTP/1 can produce method-call errors.

Supported operations listed in public docs:

- accounts;
- portfolio/account composition;
- market, limit, conditional, stop, stop-loss, take-profit orders;
- modifying limit/conditional/stop/SL/TP orders;
- canceling active orders;
- current active orders;
- available instruments;
- real-time market data;
- historical bars;
- latest market trades;
- schedules and order books;
- trade and transaction history.

## REST surface relevant to M1

REST docs list:

- `POST /v1/sessions` — exchange secret for JWT.
- `POST /v1/sessions/details` — token details and accessible accounts.
- `GET /v1/accounts/{account_id}` — account info.
- `GET /v1/accounts/{account_id}/trades` — account trade history.
- `GET /v1/accounts/{account_id}/transactions` — transaction history.
- `GET /v1/accounts/{account_id}/orders` — account orders.
- `POST /v1/accounts/{account_id}/orders` — place exchange order.
- `GET /v1/accounts/{account_id}/orders/{order_id}` — order details.
- `DELETE /v1/accounts/{account_id}/orders/{order_id}` — cancel order.
- `POST /v1/accounts/{account_id}/sltp-orders` — place SL/TP order.
- `GET /v1/exchanges` — exchange/MIC list.
- `GET /v1/instruments/{symbol}/bars` — historical bars.
- `GET /v1/instruments/{symbol}/orderbook` — current order book.
- `GET /v1/instruments/{symbol}/quotes/latest` — last quote.
- `GET /v1/instruments/{symbol}/trades/latest` — latest market trades.
- `GET /v1/assets` / `GET /v1/assets/all` — instrument lists.
- `GET /v1/assets/{symbol}/params` — trading parameters.
- `GET /v1/assets/{symbol}/schedule` — trading schedule.
- `POST /v1/report` / `GET /v1/report/{id}/info` — account reports.

REST order request fields include `client_order_id` and `comment`, which are important for our correlation and reconciliation model.

## gRPC surface relevant to M1/M2

gRPC docs list:

- `AuthService/Auth`
- `AuthService/TokenDetails`
- `AuthService/SubscribeJwtRenewal`
- `AccountsService/GetAccount`
- `AccountsService/Trades`
- `AccountsService/Transactions`
- `AccountsService/SubscribeAccount`
- `OrdersService/PlaceOrder`
- `OrdersService/CancelOrder`
- `OrdersService/GetOrders`
- `OrdersService/GetOrder`
- `OrdersService/SubscribeOrderTrade`
- `OrdersService/SubscribeOrders`
- `OrdersService/SubscribeTrades`
- `OrdersService/PlaceSLTPOrder`
- `MarketDataService/Bars`
- `MarketDataService/SubscribeBars`
- `MarketDataService/SubscribeQuote`
- `MarketDataService/SubscribeOrderBook`
- `MarketDataService/SubscribeLatestTrades`
- `AssetsService/*`
- `ReportsService/*`

Likely design:

- REST is good for M1 read-only export and simple smoke checks.
- gRPC is attractive for own order/trade streams and JWT renewal.
- WebSocket can be used for market data if gRPC market data is not sufficient or simpler.

## WebSocket surface relevant to M2

Public AsyncAPI says the WebSocket supports subscriptions for:

- quotes;
- trades;
- order book;
- bars;
- orders.

Client sends JSON `subscribe`, `unsubscribe`, or `unsubscribe_all`; server replies with envelopes:

- `DATA`
- `ERROR`
- `EVENT`

Payload types include bars, order book, quotes, trades, and orders.

Auth can be passed via Authorization header or token in subscription payload.

## Open questions for Finam/support

1. Exact MOEX futures symbols and MIC values for RI, IMOEXF, and USDRUBF.
2. Whether `client_order_id` is accepted and returned consistently for futures orders, trades, and stream events.
3. Whether market futures orders are supported for the target account type.
4. Exact SL/TP semantics for futures:
   - stop-market;
   - stop-limit;
   - take-profit;
   - good-till-date;
   - cancel/replace behavior.
5. Whether partial fills are emitted as multiple trades and whether cumulative filled qty is included in order stream.
6. Historical account trades depth/limits/pagination.
7. Whether current-session trades and historical trades are unified or separate.
8. Stream replay behavior after reconnect.
9. Maintenance-window behavior and expected error codes.
10. Whether reports include commissions/fees suitable for net PnL.

## 2026-06-27 initial token check

An initial auth smoke was attempted with the token identifier visible in the Finam portal row. The API returned:

```text
HTTP 401
Api token could not be verified
```

Interpretation:

- The visible token id is not sufficient as the secret-token value, or the token is not active yet.
- Unauthenticated read-only calls such as `clock`, `exchanges`, `assets`, and `bars` also returned `Missing auth token`.
- Next check requires the actual secret token generated by the Finam token portal after the account/data update is active.

Safe local command once the real secret is available:

```bash
FINAM_SECRET_TOKEN=... cargo run -p broker-cli -- finam-auth-check
```

The command prints only HTTP/auth shape and JWT length, not the JWT itself.

REST read-only calls use `Authorization: Bearer <jwt>`. WebSocket auth is kept
separate because FINAM async docs allow token placement in headers or
subscription payloads depending on subscription type.

Read-only surface added for M1:

```bash
FINAM_SECRET_TOKEN=... \
FINAM_ACCOUNT_ID=... \
FINAM_SYMBOL='TICKER@MIC' \
cargo run -p broker-cli -- finam-readonly-check \
  --start-time 2026-06-01T00:00:00Z \
  --end-time 2026-06-27T23:59:59Z \
  --limit 1000 \
  --output tmp/finam-readonly-redacted.json
```

The read-only probe calls only diagnostics/reference/history endpoints:

- token details;
- clock, exchanges, assets, first page of active `all_assets`;
- optional account, account orders, trades, transactions;
- optional asset, asset params, schedule, last quote, latest trades, bars.

It prints redacted JSON shape/keys instead of raw JWT or full broker payloads.
It does not place, cancel, replace, or modify orders.
When `--output` is provided, it saves only those redacted records in fixture
format `finam-readonly-redacted-v1`.

Typed read-only smoke added for M1.4:

```bash
FINAM_SECRET_TOKEN=... \
FINAM_ACCOUNT_ID=... \
FINAM_SYMBOL='IMOEXF@RTSX' \
cargo run -p broker-cli -- finam-typed-readonly-check \
  --start-time 2026-06-26T00:00:00Z \
  --end-time 2026-06-29T23:59:59Z \
  --limit 10 \
  --output tmp/finam-typed-readonly-redacted.json
```

The typed smoke validates DTO decoding and mapper conversion to broker-core for
read-only account, order, trade, quote, latest trade, and bar data. It emits
counts and boolean flags only; it does not print raw account/order/trade values.

Implementation notes from the first review:

- request/response structs containing secret/JWT values must not derive raw
  `Debug`; `AuthResponse` has redacted debug output;
- JWT is represented as an `AccessToken` newtype with redacted `Debug` and
  `Display`; it intentionally does not implement `Serialize`;
- FINAM secret-token input is represented as `SecretToken` with redacted
  `Debug` and `Display`;
- non-2xx REST error bodies are redacted by default: the error keeps HTTP
  status, JSON body kind, sanitized top-level JSON key metadata, body length,
  and SHA-256 hash, but not the raw body;
- CLI output uses redacted error presentation for transport errors, avoiding URL
  leakage by default;
- FINAM errors expose a structured `FinamErrorKind` for retry/backoff/readiness
  decisions without parsing redacted strings;
- response JSON decode failures use `FinamErrorKind::Decode`, not transport
  HTTP classification;
- read-only CLI obtains JWTs through `FinamAuthManager`, which caches the token
  and refreshes before the public 15-minute lifetime expires;
- REST client requests have a bounded timeout, default 10 seconds;
- `auth()` and `token_details()` use the same `rest_url()` path builder as other
  REST endpoints;
- FINAM API capabilities are separate from gateway-enabled features;
- Phase 1 enabled features keep live orders, stops, SLTP, and brackets disabled;
- M3a-4 includes dry FINAM order request builders for MARKET/LIMIT/CANCEL path
  and body fixture tests only; no REST order POST/DELETE send methods are
  implemented or enabled;
- M3a-5 restricts dry order request builders to preflight-approved marker
  types, records only redacted mock-client diagnostics, and adds mock-only dry
  ACK Redis publication without FINAM order endpoint calls;
- M3a-6 adds an approved-only mock execution boundary and simulator tests for
  accepted/rejected/timeout outcomes, still without FINAM order endpoint calls;
- M3a-7 adds accepted-without-broker-id reconciliation policy and dry cancel
  execution simulation, still without FINAM order endpoint calls;
- M3a-8 adds dry client-id recovery, cancel accepted broker-id mismatch policy,
  and source-scan boundary coverage, still without FINAM order endpoint calls;
- raw `serde_json::Value` is acceptable only for the shape probe. Typed DTOs and
  mappers are required before Redis gateway/readiness work.

## 2026-06-29 real read-only characterization

Real local FINAM auth and read-only probes were run with a git-ignored `.env`.
No raw secret, JWT, account payload, or order payload was committed.

Observed:

- auth succeeds and returns JWT;
- account endpoint is reachable and reports an active union account;
- account positions shape was empty during the probe;
- account orders shape included one canceled IMOEXF limit order;
- account trades and transactions require an interval; with interval they
  returned typed shapes successfully;
- typed DTO/mappers were validated with `finam-typed-readonly-check` against
  token details, exchanges, assets, account, orders, trades, transactions,
  asset params, schedule, quote, latest trades, and bars;
- typed smoke classifies order snapshots into active, terminal, and
  blocking-unknown status groups;
- broker/manual `client_order_id` values longer than the core FINAM-safe
  20-character id are tolerated in read-only order mapping by leaving
  core `client_order_id` empty for that record while preserving a redacted
  broker-native fingerprint;
- the correct FINAM symbol for IMOEXF is `IMOEXF@RTSX`;
- `IMOEXF@MOEX` returns not found;
- `GET /v1/exchanges` is the working exchanges endpoint;
- M1 bars with `TIME_FRAME_M1` must use a short interval; a 2026-06-26 to
  2026-06-29 interval succeeded for `IMOEXF@RTSX`.

Bar timestamp convention remains unproven. The current mapper treats
`bar.timestamp` as `open_ts` and derives `close_ts = open_ts + timeframe`, but
runtime/live bar consumption must wait for a golden test proving FINAM's
timestamp convention across normal bars and session gaps.
See `docs/finam-bar-finality-golden-test-plan.md` for the required acceptance
checks before any runtime consumer uses historical bars for execution decisions.

The redacted shape fixture remains local under `tmp/` and is git-ignored.
