# Signal-shadow: минимальное дополнение auth lifecycle R0

Статус: SOURCE REVIEW CANDIDATE. Установка и observation НЕ разрешены этим документом.
Baseline: `3ac8cbf859a0433221b54d797f033cf7113bc86c`, принятый S5.
Стратегия, feedback, seed, finality и торговые поверхности не меняются.

## Причина

Read-only проверка 24 октября подтвердила `readonly=true`, RTSX без задержки
и срок access token примерно 15 минут. Один токен до EOD не является
выполнимым контрактом сессии. Необходимо обновлять авторизацию, а не подменять
expires_at или отключать проверку срока. Генерировать токен на каждый GET не нужно.

## Выбранный счёт

Источником является существующий `FINAM_ACCOUNT_ID` в локальном `.env` проекта.
Получено точное совпадение с одним из двух account_ids в session details;
GET выбранного счёта вернул совпадающий account_id, `ACCOUNT_ACTIVE`, тип `UNION`.
Это проверка API, не утверждение о просмотре UI кабинета или проверке баланса.
ID из старых конфигураций P0 не совпадает с выбранным и не переносится.
Точный выбранный ID записан в локальный `selected-account.private.json` (0600),
вне Git/ZIP; при последующей подготовке он войдёт в приватный RunPlan.account_id.
Полный RunPlan и оперативные входы ещё не собраны и на VPS не установлены.

## Источник контракта FINAM

Проверено 24 октября 2026:

- https://api.finam.ru/docs/rest/ — `/v1/sessions`, `/v1/sessions/details`;
- https://api.finam.ru/docs/async-api/ — токен при соединении и в SUBSCRIBE;
- https://api.finam.ru/getting-started/bystryy-start/poluchat-dannye-cherez-websocket/
  — ждать HANDSHAKE_SUCCESS перед SUBSCRIBE.

Отдельного документированного re-auth уже открытого WS в рассмотренном
контракте нет. Поэтому применяется обычное новое соединение и повторная
подписка с новым токеном. Это наш явно выбранный способ ротации, не обещание
FINAM сохранить подписку при изменении файла. gRPC renewal не добавляем.

## Реализация

`AuthLifecycle` использует существующие `FinamRestClient.auth` и
`token_details_typed`, `SecretToken`, `AccessTokenLease`. Отдельный constructor
клиента закрепляет HTTPS api.finam.ru, запрещает proxy и redirects, timeout 10s.
Существующие клиенты и их policy не изменены.

Один shared owner для REST/BARS/QUOTES, mutex обеспечивает single-flight.
Проверка каждого нового grant: непустой токен, `readonly == true`, точное
наличие выбранного account_id, RTSX/zero-delay/DEPTH_OF_MARKET, parse фактического
expires_at. Остаток должен быть >150s и <=24h; обновлённый срок обязан продвинуться.
Margin 120s; дополнительная 1s защита используется только в monotonic lease,
не изменяя сохранённый provider expires_at. Проверяются wall + monotonic clock.
Обратный скачок wall clock, отказ auth/details, неподходящие права/счёт/срок,
timeout или отмена обновления приводят к терминальному HOLD без retry/fallback.
Общий auth timeout 25s; максимум 128 поколений за bounded сессию <=24h.

REST берёт актуальный grant перед каждым запросом. Пока grant пригоден,
новый POST не делается. После ответа срок повторно проверяется до публикации.
Запросы данных остаются в существующем GET-only клиенте.

WS-worker просыпается не позже renew_at даже без входящих кадров. Таймер
прерывает только cancel-safe ожидание следующего кадра, не send SUBSCRIBE и
не внешний budget future. Старый сокет закрывается локально; новое соединение
получает актуальный токен в header и после HANDSHAKE_SUCCESS в SUBSCRIBE.
Каждый worker отдельно переходит на новое поколение. Краткое пересечение
старого ещё действительного grant с новым разрешено; просроченный кадр не
публикуется. Ошибки соединения, подписки или silence остаются терминальными.

Auth reconnect сохраняет protocol/quote dedup/frontier и старую silence anchor;
модель/feedback/REST sequence не пересоздаются. Auth reconnect не потребляет
единственный controlled BARS reconnect. Пересечение срока с самим controlled
reconnect приводит к явному отказу, не к ложному S5 успеху. Физические поколения
соединений растут независимо от auth generation; старые Connection отклоняются.

Управляемая остановка сохраняет общий stop latch, abort + bounded drain всех
workers и terminal output. SIGINT отменяет также ожидающий auth future.

## План и credentials

RunPlan вместо operator-supplied token_expires_at требует точную policy
`provider_expiry_readonly_rtsx_v1`. Полнота расписания до EOD, config fingerprint,
latency и account binding сохраняются. Старый plan отвергается при parse;
непреднамеренная подмена credentials недопустима.

Аргумент observe теперь `READ_ONLY_SECRET_FILE`, не файл access token.
Только явно переданный regular file, на Unix без group/other permissions;
ожидаемое последующее deployment — systemd LoadCredential. Env-discovery нет.
Секреты/токены не входят в diagnostics, вывод ошибок, evidence или argv.
Новая установленная конфигурация и выдача secret в service требуют отдельной
installation review; действующий unit/credential не изменялись.

## Диагностика и проверка

`auth_validated`: только generation, фактический expires_at и булевы результаты
проверок. `signal-shadow-auth-ws-v1`: отдельные BARS/QUOTES subscription-sent и
first validated DATA после auth reconnect, с connection/auth generation.
Это НЕ provider ACK и НЕ finality. S5 receipts используют прежний schema;
auth events не считаются controlled S5 qualification. Raw старые QUOTES/RunPlan
по-прежнему требуют приватного хранения и не становятся public evidence.

Тесты: single-flight/cache, ротация, refresh failure, readonly/account/permission
drift, expiry, cancellation latch, WS timer без порчи budget, новый токен в
SUBSCRIBE, сохранение quote dedup, отдельный controlled reconnect budget,
silence/stale Connection/faulted state. Существующие model/parity/regressions
должны пройти без изменений стратегии. Тесты не являются real provider rotation.

Будущий отдельно разрешённый bounded run должен покрыть минимум две auth
generation и успешные subscription-sent + свежие DATA обоих каналов после
реальной ротации, не выйти за stop/latency/silence/output budgets. Отсутствие
этих событий — INCOMPLETE, не ACCEPT. Сначала source review, затем release и
installation, затем согласование окна/stop. FINAM execution остаётся CLOSED.
