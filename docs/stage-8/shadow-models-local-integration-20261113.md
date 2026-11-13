# Три shadow-модели: общий локальный M10/persistent slice

Статус: SOURCE REVIEW CANDIDATE, не deployment/execution acceptance.
Baseline численных моделей: 017698205c482087334ac6c4d13fd9b65a19422d.
Дополнительные reviewer-regressions приняты в fdfcd4a61c9cca836dd2ca07d58469f6391915cc.
Работающий FINAM V6 не связан с этим standalone crate и не изменяется.

## Достаточная граница этой поставки

Одна явно выбранная сессия, три инструмента, шесть независимых policy-веток.
Согласованный владельцем проекта scope расширяется с IMOEXF на USDRUBF и RI.
Общий вход проверяет mapping, номинальные границы M10, OHLCV/tick/lot,
source sequence, исходные millisecond clocks и версию построения свечи.
Вход, два результата/набора модельных marks данного инструмента, два checkpoints
и history-head сохраняются одной SQLite-транзакцией. Другие инструменты не
переигрываются и не меняются. Недостаточный контекст одной ветки означает
явный not-ready без marks, а не вымышленную готовность или остановку остальных.

Все входы этой поставки имеют единственный evidence-kind:
`declared_clock_replay`. JSON, raw hash, catalog hash, watermark и имена
инструментов здесь являются декларациями, а не FINAM admission capability.
Нельзя выдавать эту поставку за подключение новых инструментов к рынку,
истинные prospective receipt clocks, допуск VPS или clean observed session.
Нет ни FINAM-клиента, ни credential, ни quote, ни Redis, ни брокерного исполнения.

## Mapping и источник

Точный FINAM mapping из принятого контура известен только для IMOEXF@RTSX:
tick 0.5, lot 1. Для USDRUBF и RIZ6 / RTS-12.26 владелец подтвердил отсутствие
созданного catalog/export. Их ID нельзя угадывать по модельному имени.
В qualification используются только `fixture:IMOEXF`, `fixture:USDRUBF`,
`fixture:RIZ6`. Режим declared_catalog_snapshot всё ещё не даёт operational
authorization: независимая каталоговая проверка предстоит отдельно.
Модель RI закреплена за RIZ6; автоматической смены контракта нет.

ALOR handoff использует внешние M10. Действующий FINAM V6 получает M10 из
проверенных M1. Это не объявляется эквивалентностью по одной метке времени.
Общий контракт различает external_m10_v1 и sparse_m1_v1; construction фиксируется
в generation binding и не может незаметно переключиться.
В sparse_m1_v1 проверяются точный диапазон, объявленная полнота ответа, 1..10
уникальных упорядоченных M1, OHLCV и точная decimal-агрегация. Сохраняются
номинальные границы M10, open первой фактической M1, high/low всех фактических,
close последней, сумма volume. Пропуски не заполняются; пустой/обрезанный,
неупорядоченный, конфликтный или неверно агрегированный вход отвергается.
Этот валидатор не заменяет принятый V6 producer и его доказательство получения
ответа. raw_sha256 остаётся ссылкой: без raw ответа он не доказывает источник.

## Время, seed и модель

В журнале и экспорте остаются исходные start/end/received/durable/source_processed
в миллисекундах. begin label никогда не означает время доступности.
Для неизменённых численных kernels используется проекция ms / 1000:
она сохраняет неравенства закрытия и целосекундного cutoff, но не подменяет
исходные evidence clocks. local_write_prepared_at_ms — реальное локальное время
подготовки SQL-записи, НЕ broker receipt и НЕ точный момент fsync/commit.

Manifest фиксирует исходную историю, cutoff watermark, freeze и implementation
fingerprint. В рамках declared-clock replay watermark задан извне; он не
вычисляется из последней свечи. Часы history не переносятся к cutoff:
полученный после cutoff материал не становится своевременным anchor.
Freeze/два предыдущих anchors и политики контекста остаются принятыми.
Временной диапазон single-session генерации ограничен закреплённым календарём
после 14.07.2026 и сроком RIZ6, с исключением 04.11.2026; это не новый биржевой
календарный сервис. Для более ранних дат действуют отдельные numeric fixtures.

Model prefix начинается с 07:00 МСК, строго последовательно до 23:40 (101 M10).
Отсутствующую M10 нельзя создать из последней цены. Late OHLCV correction не
перезаписывает ранее использованный вход; конфликтующий дубль отвергается.
Исторические catch-up, текущие network receipts и rollover следующего дня не
разрешаются под видом этого replay. Новый день нельзя начать cold-flat при
незавершённой позиции/pending: continuous runner потребует отдельной интеграции.

Численные файлы context/session/IMOEXF/USDRUBF/RI byte-for-byte сохранены.
Profiles IMOEXF/USDRUBF скопированы из handoff без изменений; RI уже закреплён.
provenance.json сохранён как историческая provenance численного порта, включая
старую метку RI candidate и описание удаления upstream filesystem helpers.
Он не описывает добавленный local/ SQLite writer; текущая граница — этот документ
и явный local module header. Принятый RI статус закреплён отдельным acceptance JSON.
Стратегии, fee policies, очередность действий и feedback не изменены.
RI внутренний DataOrigin::Live нужен существующему numerical facade как selector;
outer evidence всегда declared_clock_replay, ни один receipt от этого не
становится observed. Модельные marks — не брокерные ордера/исполнения.
Ready, full-grid-flat и observed-clean — разные поля. Естественная сделка
не нужна для PASS инфраструктурного сценария.

## SQLite и restart

Один локальный файл на generation; create_new не перезаписывает существующий.
SQLite bundled, journal_mode=DELETE, synchronous=FULL, foreign_keys=ON,
busy timeout 2s. Родительская директория sync после создания. Файл создаётся
0600 на Unix. Во время транзакции SQLite может создать transient rollback journal.
Это ограниченный session journal, не новый recovery service/framework и не DB0.
Устойчивость к SIGKILL доказывается тестом; потеря питания/диска этим не доказана.
IO-ошибка commit делает экземпляр непригодным до reopen, без попытки угадать исход.

Дубль instrument/sequence с тем же canonical input даёт AlreadyCommitted и
стабильные result IDs, но не возвращает marks как новую доставку.
Конфликтующие bytes/hash или gap дают ошибку без частичной фиксации.
Source sequence включает seed-префикс; текущие 101 inputs продолжают его.
Каждая SQL-запись связывает input, outputs, checkpoint digests и цепочку history.
При reopen проверяются bounded causal log, два результата на input, semantic
replay всех исходных данных, latest checkpoint/result frontier и независимый
restore сохранённых numerical checkpoints. Повреждённый файл не сбрасывается.

В принятых numerical facades restore сдвигает bookkeeping clock checkpoint.
Поэтому checkpoint bytes/hash после restart не обязаны совпадать с uninterrupted
процессом. Проверка causal replay исключает ТОЛЬКО checkpoint_sha256 из сравнения
outcomes; state, marks, result IDs, original inputs/clocks и остальные поля
обязаны совпасть. Настоящий сохранённый checkpoint всё равно проверяется по
своему checksum, связывается с latest outcome и восстанавливает точный expected
state. Никакого исключения для численного состояния или marks нет.

Hash chain выявляет повреждение и несогласованность, но не является подписью
или защитой от владельца БД, способного целиком переписать журнал.
Одновременный устаревший writer/export требует reopen. Экспорт — исторический
readback; нет очереди отправки marks и нет exactly-once внешней доставки.

## Воспроизведение

Standalone manifest: crates/finam-shadow-models/Cargo.toml.
Команды cargo test (debug/release), strict clippy и fmt выполняются отдельно
от большого workspace. Корневые Cargo/CI/authority и accepted V6 не меняются.

```sh
cargo build --release --locked --manifest-path crates/finam-shadow-models/Cargo.toml
python3 -B scripts/finam_shadow_local_session_verify.py \
  --binary crates/finam-shadow-models/target/release/finam-shadow-local-session \
  --output /absolute/new-directory
```

CLI: implementation; create/export MANIFEST DB; apply MANIFEST DB CANDLE;
replay MANIFEST NEW_DB CANDLES. Replay ограничен одной сессией/303 inputs.
Fixture generator создаёт synthetic 26.10, а не выдаёт их за данные 13.11.
Численный oracle остаётся прежним: scripts/finam_shadow_models_parity.py
для IMOEXF/USDRUBF/RI и дополнительный reviewer RI regression.

## После source review

1. Получить read-only FINAM catalog/mapping для USDRUBF и RIZ6, закрепить
   instrument IDs, tick/lot/contract и проверить исторические/WS M1/M10.
2. Подключить общий обработчик к настоящему admitted producer (не JSON forge),
   с честными receipt/watermark clocks и явным разделением replay/observed.
3. Принять минимальный continuous-session/rollover bridge, исключив потерю
   позиции/pending. Затем Linux release и bounded deployment/execution gate.

Это дальнейший интеграционный остаток, а не новый аудит принятых формул.
Operational P0/V6, FINAM POST/DELETE, реальные ордера, DB0 и runtime-live закрыты.
