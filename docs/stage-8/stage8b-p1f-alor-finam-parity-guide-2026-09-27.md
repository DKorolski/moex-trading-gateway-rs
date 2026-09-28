# Руководство разработчику Finam: IMOEXF parity

## 1. Разделение ответственности

Сохранить signal engine и стратегию HybridIntraday, заменять transport adapter,
а не переписывать модель одновременно с подключением брокера. Финансовая модель
не должна зависеть от номера портфеля, Redis stream ID или broker order ID.

Рабочая цепочка: normalized market data -> deterministic model state -> intent ->
execution policy -> broker adapter -> ack/fill/position reconciliation.

В Alor action-scoped CWS означает свежий авторизованный control channel для
действия. В Finam НЕ надо имитировать CWS как протокол. Надо сохранить семантику:
готовая авторизованная сессия перед send, bounded retry, явный uncertain outcome,
reconcile до повторной отправки. Timeout не равен гарантированному неисполнению.

## 2. Market-data contract

Использовать одинаковые закрытые 10m OHLCV, не пересобирать молча из 1m.
В `fixtures/imoexf_raw_10m_msk_utc.csv`:

- `bar_start_msk`: биржевая метка начала свечи в MSK;
- `bar_start_utc`: та же метка в UTC;
- `available_at_utc`: начало + 10 минут, ранняя допустимая обработка closed bar;
- open/high/low/close/volume: raw audit, включая service/weekend bars.

Нельзя назвать начало бара его временем получения и входить на 10 минут раньше.
В Alor поле `close_time_utc` исторически переносит модельную метку свечи:
имя поля само по себе не доказывает timestamp convention.

До 14.07 исключать pre-09:00; после этой даты baseline использует 07:00,
candidate 09:00. 06:50 auction/service остаётся только в raw audit. Weekend-state
candidate обновляет дневные anchors на доступных weekend bars, без intent.
Baseline исключает выходные полностью. Нельзя брать чужие anchors/ledger между ними.

Каждый replay начинается с warmup (в пакете с 01.07). На текущей сессии нельзя
использовать её будущие high/low/close для prev-day context. История прогревает
индикаторы, но не должна отправлять старые ордера в брокер после restart.

## 3. Signal и execution parity проверяются отдельно

Сначала baseline07 BO-only, затем candidate09 BO-only. Для каждой даты и бара
логировать: profile/version, symbol, bar label, available/received time,
previous-session dates, prev_close, prev_range, prev_return, BO thresholds,
wait eligibility, side-used flags, owner, position, pending lifecycle и decision.

Сверять по очереди:

1. Data parity: состав, OHLCV, календарь, timestamp, duplicates/gaps.
2. State parity: anchors, уровни, session clock, прогрев, reentry flags.
3. Signal parity: enter/exit/suppress, side, model timestamp, reason.
4. Execution parity: send/accept/fill times, qty/partial fills, prices, fees.
5. Recovery parity: restart, uncertainty, cancellation, final broker-flat.

В source найти `hybrid_intraday_runtime.rs`, `hybrid_intraday/orchestrator.rs`,
`hybrid_intraday/intraday_breakout.rs`, `runtime.rs` и `strategy_adapters.rs`.
Не переносить только выражение K*range, пропуская ownership и side-used state.
Rust reference source и patch лежат отдельно; unpatched HEAD не знает нового
paper close-fill флага. Архив не содержит готовых credentials или среды Finam.

## 4. No-overnight

BO выход по closed 23:30 bar, фактически около 23:40 MSK. Запретить новые входы
на/после EOD. Paper candidate исполняет exit на текущем close, не на утреннем open.
Не завершать replay с переносом позиции и не закрывать её задним числом на
последнем баре после обнаружения разрыва следующего дня.

При отсутствии позднего бара статус проверки FAILED/INCOMPLETE, а не PASS.
В live для feed outage нужен отдельный проверенный clock/flatten safety mechanism;
данный freeze не утверждает, что bar-driven модель гарантирует flat без данных.
В эмуляторе сохранять event time и фактический callback/send time раздельно.

## 5. MR выключен не только на уровне брокера

`live_mr_entries_enabled=false` должен подавить создание MR entry до ownership
и pending state. Иначе скрытый MR будет блокировать BO и BO-only не получится.
High180 shadow ledger в текущем Alor живёт отдельно от исполняемого BO. Его
состояние не должно блокировать BO из-за выключенного MR. Для нового BO-only
candidate gate disabled, seed не нужен. Для точного переноса lb120 accounting
потребуется отдельный ledger export/bootstrap; это не часть BO signal parity.

Не включать TP/SL brackets в BO. В текущем IMOEXF BO входы/выходы Market;
broker brackets относятся к MR, который в этом freeze отключён.

## 6. Обязательные сценарии

- Weekend обновляет candidate anchor, но не генерирует сделку; baseline пропускает.
- MR-сигнал при выключенном MR не занимает ownership и не подавляет BO.
- Дубликат/перестановка баров и restart не повторяют уже исполненный intent.
- 23:30 exit заканчивается flat в тот же день, без требования следующего бара.
- Missing evening bar приводит к видимой ошибке качества/сигналу безопасности.
- Partial fills 1+5 и 3+3 при qty6 не считаются отдельными позиционными кругами.
- Terminal order ack не подменяет подтверждённый broker position quantity.
- Late fill после cancel/timeout не создаёт повторный полный exit и reversal.
- Timeout после send не приводит к слепому дублю Market.
- Reconnect/restart восстанавливает pending exact request ID; stale IDs не висят.
- Инструменты общего портфеля фильтруются по instrument identity и ownership.
- Новая брокерская идентификация IMOEXF не меняет tick=0.5, point multiplier=10.

## 7. Воспроизводимый запуск

В каталоге `research`:

```bash
python3 -m unittest discover -s imoexf_no_overnight_audit_2026_09_27 -p 'test_*.py' -v
python3 imoexf_no_overnight_audit_2026_09_27/run_audit.py
```

Зависимости: pandas, pyarrow, matplotlib. При наличии frozen parquet сеть не нужна.
Для сравнения сигналов использовать zero-slippage CSV; для экономики отдельные
сценарии 0/0.5/1 пункт на market side. Broker tariff и bid/ask Finam не подгонять
под Python fee proxy: фактические затраты показывать отдельной колонкой.

Для Rust исходники извлечь в новый каталог, положить `source/Cargo.lock` в
`alor-rs-main/Cargo.lock`, при необходимости применить scoped patch. Он рассчитан
на reference HEAD из `freeze_metadata.json`. Не применять поверх чужого dirty tree.
Новый TOML из `candidate/` положить в `alor-rs-main/configs/`: это новый файл,
его нет в HEAD archive. Затем выполнить `cargo test -p strategy-runtime --lib`
и `cargo test -p strategy-runtime --test config_tests -- --test-threads=1`.
Полные integration/replay suites могут требовать внешние артефакты исходного
репозитория; этот пакет не подменяет весь dev workspace.
Candidate собран native cross-compiler Rust 1.96.1 для x86_64-unknown-linux-musl;
builder image закреплён digest в Dockerfile. Release binary и image ID фиксируются
отдельно. Patch также запрещает новые same-day entries на/после 23:30, сохраняя
обработку выхода. Это изменение поставлено только в новый shadow image, не live.

## 8. Отчёт и gate

После экспорта Finam в общий формат можно выполнить:

```bash
python3 compare_rounds.py --expected fixtures/candidate09_python_reference_trades.csv --actual finam_candidate09.csv --output candidate09_diff.json
```

Обязательные колонки actual: component, side, entry_bar, exit_bar, entry_price,
exit_price, exit_reason. Здесь entry_bar/exit_bar имеют тот же строковый формат
MSK candle-start, что fixtures; заранее привести timestamps и reason vocabulary.
Сравниваются позиционные круги, не отдельные partial fills. Самопроверка файла
с собой проверяет helper, но не доказывает Rust/Finam parity. Для baseline указать
baseline07 fixture. Missing/extra и signal/contract drift отделены от price drift.

Одна строка на decision/round: profile, session_date, source bar, owner, side,
signal_ts, send_ts, accepted_ts, fills, qty, exit_reason, broker_flat_ts, pnl,
drift_class. Классы: data, anchor, signal, execution_timing, tick_rounding,
fee/slippage, lifecycle, bo_gap_flatten, unresolved.

PASS требует отсутствия необъяснённых signal/state расхождений на одинаковых
данных, нулевых weekend/MR/broker emissions в запрещённых режимах, нулевого
overnight и успешных lifecycle/restart тестов. Совпадение суммарного PnL само
по себе НЕ PASS. Сейчас статус пакета: Python reference есть, полный
trade-by-trade Rust/Finam parity PENDING. До него только offline/paper/shadow.
