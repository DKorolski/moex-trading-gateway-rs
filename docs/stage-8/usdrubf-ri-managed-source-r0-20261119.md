# USD/RI managed V8 source composition R0

19 ноября 2026. Scope: source review, не Linux release и не разрешение запуска.

Accepted parent: `342a3cbf7a2720e0cbd09bc014192fef646ffedb`.
Его SRC-1/SRC-2 не переоткрываются; параметры, комиссии и численные алгоритмы
шести моделей не меняются. Сохранены точные USDRUBF/RIZ6 IDs, tick, trade lot,
reference/contract sizes, expiry и запрет rollover.

## Необходимый срез

`Prepared::into_managed` создаёт два настоящих типизированных PrimaryProcessor
и два существующих V8 Coordinator. Это не переименование старого IMOEXF Hybrid
runtime: ObservedSession/close-fill book IMOEXF здесь вообще не создаётся.
Два отдельных admitted calendar, physical subscription owner, generation,
bootstrap, REST-role scheduler и recovery frontier. Четыре независимые
USD/RI arms получают поток своего инструмента; две IMOEXF arms не активируются.

Перед сетевой работой exact account SHA должен совпасть с принятым Plan и
обоими market-only clients. Переиспользуется один AuthLifecycle с принятой
readonly/account/permission проверкой каждого REST/WS grant и обновлением
действительного срока. Ни secret, ни token, ни plaintext account не пишутся
в control/evidence. Новых auth/network origins и route allowlists нет.

V8 continuous driver выбирает instrument из native processor и требует тот
же selector у GET client. Старый IMOEXF путь остаётся IMOEXF. Для USD/RI
используются прежние heartbeat, subscription lease, bounded ingress,
bootstrap, managed reconnect/resubscribe, same-connection historical repair
и REST recovery. Не меняются лимиты: U100, BO 2000ms, 3 episodes,
3 transport attempts, 45s episode, четыре recovery reads.

## Общий сериализованный барьер

Оба blocking writer actor обращаются к одному Store/SQLite connection через
один mutex; это не два SQL writer и не async callback в транспортном цикле.

1. Проверить STOP/deadline, UTC/monotonic и native owner/frontier/generation.
2. Для live input проверить bound healthy continuity gate.
3. Сохранить полный admitted REST source, если нужен historical replay.
4. Синхронно записать publication intent с точной source projection.
5. Выполнить Journal.apply и проверить durable reread: Candle, revision,
   два result IDs/checkpoints.
6. Повторить STOP, native admission, continuity и latency/replay deadline,
   синхронно записать checked-readback witness, снова проверить границы.
7. Не отпуская общий mutex, consume original native capability и продвинуть
   только соответствующий source frontier — последняя операция publication.

JSON не превращается в native M10/recovery capability. REST и WS historical
inputs сохраняют исходные replay receipts/clocks и не объявляются WS_LIVE.
Часы numerical Candle остаются DECLARED availability, не fsync timestamp.
Поздняя запись/STOP/loss после SQL оставляет HOLD с SQL-ahead prefix, а не
повторный callback. Автоматического restart/adoption этого prefix нет.
Последующие транспортные диагностические события не являются вторым callback.

## STOP и matching reader

Два V8 futures выполняются в одной явной композиции без detached outer tasks.
Окончание одного останавливает root STOP другого. Drop running composition
тоже ставит STOP. Каждый существующий driver дренирует HTTP/socket и join
свой blocking writer в прежнем stop budget 1000ms. Новый aggregate finish
пишется после возврата обоих drivers; сведения о drain не выдумываются.
Занятый/poisoned Store после join budget не ожидается безгранично:
completion отсутствует, результат INCOMPLETE. Нельзя принимать это как PASS.

`verify-usd-ri-managed OUTPUT_DIRECTORY` — только чтение файлов, feature
`models-bridge` обязателен. Отдельный versioned managed-control не ослабляет
принятый gen-1 component reader. Reader перепроверяет точный parent contract,
metadata/calendar/configs, full HTTP snapshots с original clocks/hash,
per-source prefixes/generations, canonical aggregation, SQL/results linkage,
final actual frontiers и обе completion/HTTP/drain/recovery цепочки.

`MANAGED_PREFIX_VERIFIED` означает согласованный bounded source-library prefix,
не observed clean session, не EOD, network/systemd/cgroup или ALOR parity.
Отсутствующий post-join completion — отказ; корректно удержанный prefix — HELD.
Native capability или permission из файлов reader не выдаёт.

## Проверки и границы

Тесты только synthetic; локальные Duplex WS действительно проходят оба V8
driver/writer/STOP/join пути, но не являются FINAM/network evidence.
Отдельно проверяются native REST/WS recovery, независимость frontiers,
post-SQL STOP/late/continuity fault, exact account до сети, no-resume,
read-only inspection и rehashed semantic mutants.
Сохраняются прежние component/reference negatives и numerical ALOR vectors.
Точные итоговые counts находятся в immutable package checks, не в обещании.

VPS, installed IMOEXF, P0/O3, DB0, timers, credentials, real orders, main/origin
не менялись. Authority closure намеренно не подменяет source review.
Следом: review этой композиции; затем необходимый executable/input glue
и отдельная exact Linux/native qualification перед USD/RI execution package.
Многосессионный soak, restart и перенос на live этим срезом не разрешены.
