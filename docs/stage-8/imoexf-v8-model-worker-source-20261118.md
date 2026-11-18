# IMOEXF V8 model worker — bounded source slice, 18.11.2026

Baseline: `1f697365a3d16952cadbb577b21812d11c426d0c`.
Scope: feature `models-bridge`, explicit library constructor
`Coordinator::new_with_imoexf_models`. This is a SOURCE candidate, not an
installation, executable activation or clean observed-session claim.

## Что сделано

Один native V8 producer передаёт одну и ту же допущенную M10 контрольному
baseline observer и двум независимым IMOEXF policy arms принятого numerical
facade: Author41-short + Hybrid BO053. Два arms — две context policies, а не
отдельные MR/BO книги. Параметры, numerical kernels и ALOR reference не меняются.
USDRUBF/RIZ6 arms сохраняют исходные checkpoints и не получают выдуманных данных.

Пути доставки: обычная `WS_LIVE`, `REST_EXCEPTION` (включая sparse), historical
REST recovery и `WS_RECOVERY_BACKFILL`. Последние два остаются Replay, не Live.
Вход — только borrowed native capability. JSON/экспорт не создаёт capability.

Порядок writer:

1. Проверка native owner, generation, frontier и действующего deadline.
2. Контрольный observer публикует своё baseline-состояние. Его close-fill feedback
   НЕ передаётся новым arms: они используют собственное принятое next-open
   виртуальное исполнение.
3. Проверка общего STOP/бюджета; атомарный SQLite input + два outcomes + два
   checkpoints; exact durable readback.
4. Durable evidence общего barrier, повторная проверка фактического времени/STOP.
5. Только затем `committed` / `recovered` / `recovered_ws` продвигает источник.

Ошибка любого consumer останавливает общий writer. Если SQL уже committed,
результат сохраняется, source frontier не продвигается, повтор callback запрещён.
Нет распределённой транзакции между baseline JSONL и SQLite, и rollback уже
committed результата не имитируется.

## Binding, clocks и границы доказательства

Attachment только fresh: пустой model input log, нулевой native committed count,
frontier точно на freeze, одинаковые source config/policy и старт сессии; оба
IMOEXF contexts должны быть ready. Manifest hash и initial chain записываются
в cohort binding. При смене freeze/config/policy/implementation — отказ.

`Journal::open` остаётся проверкой/чтением immutable локальной истории.
Непустой journal нельзя автоматически подключить к новому producer даже при
совпадении следующей метки: это ещё не доказательство общего prefix. Старый V6
direct bridge exact-redelivery contract сохранён; он не разрешает native cohort
restart. Новый cohort marker явно отклоняется прежним single-journal reader.

Исторический numerical facade по-прежнему называется `declared_clock_replay`.
В его Candle `received_ms` — исходный network receipt, а legacy `durable_ms` и
`source_processed_ms` — **declared availability** из canonical_ready, не SQL fsync.
Они не переносятся на nominal close и не превращаются в live evidence. В отличие
от контрольного baseline replay, reaction facade допускает позднюю availability:
модель принимает решения/next-open fills по исходным bar labels. Все реальные
REST/WS receive/ready/episode/replay clocks сохраняются в provenance; отдельное
`local_write_prepared_at_ms` остаётся physical SQL observation, не fsync receipt.
Seed clocks не меняются. Budget/STOP проверки используют только actual clock.

Для независимой проверки native SHA сохраняются и структурные receipts, и exact
`canonical_receipt_json` / `source_receipt_json`: JSON object key order после
парсинга не обязан совпадать с исходной сериализацией struct. Их SHA относятся
к точным UTF-8 строкам, а recovery source hash — к полному typed recovery receipt.
REST evidence не подменяет WS authority; у WS historical REST evidence отсутствует.
Полный recovery GET один раз durable-сохраняется существующим native control
journal до model SQL. Model provenance хранит ссылку snapshot/evidence SHA на эту
запись, а не копирует шестичасовой raw body в каждую Candle с лимитом 64 KiB.
Для независимого анализа исходных данных нужны оба журнала; одного SQLite
недостаточно для самостоятельного доказательства native REST admission.

Полный numerical export может содержать два outcomes даже при NOT_READY в старом
локальном API. Поэтому новый attachment отдельно требует ready обоих IMOEXF arms;
одного факта SQL commit недостаточно для заявления «модели исполнились».

## Проверки

Новые synthetic integration tests проверяют два outcomes на M10, неизменность
четырёх inactive arms, retransmission/dedup, sparse REST, двухшаговый historical
REST (включая максимальный шестичасовой gap), native late WS, исходные receipts/hashes, отказ неверных bindings,
атомарный SQL failure и STOP/deadline после SQL. Проверяются source-not-advanced,
no retry и запрет single-journal aggregate EOD/restart.

Существующие V6 bridge corruption/readback/SIGKILL, V8 owner/episode/deadline и
default runtime regressions сохраняются. Точные команды, итоговые counts и raw
logs — в evidence ZIP; host checks не являются native Linux qualification.

## Что остаётся до запуска

1. Source review этого bounded среза. Нынешний executable ещё НЕ вызывает новый
   constructor: установка `models-bridge` feature сама ничего не включает.
2. Один следующий implementation slice: явный CLI/config + manifest из допущенных
   исторических inputs и единый completion contract для baseline/двух SQL arms.
   До его проверки aggregate EOD seal отсутствует намеренно. Для самого первого
   bounded запуска restart можно оставить закрытым, без нового recovery framework.
3. Exact-source Linux release, минимальная native qualification, отдельный bounded
   execution package и разрешение. После достаточного IMOEXF наблюдения — общие
   source/calendar/warmup adapters для USDRUBF/RIZ6 без смешивания RI semantics.

Работающий VPS V8 baseline, P0/O3, timers, credentials, DB0/Redis, FINAM execution,
реальные заявки, authority и origin/main этим source изменением не затрагиваются.
Ни numerical parity tests, ни synthetic source tests не доказывают текущую live
ALOR parity. Для неё позже нужны данные одной реально наблюдавшейся сессии.
