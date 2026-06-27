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

