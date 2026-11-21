# WS-primary v2 entry и evidence export — source contract

Это описание candidate source, а не разрешение установки/активации и не итоговый
ACCEPT. Полная T01–T40 matrix, source review и native qualification ещё нужны.
Старая REST-driven команда `observe` не принимает v2 и не является fallback.
Стратегия baseline07 BO-only no-riskgate и candle-close feedback не меняются.

## Явные команды

```text
finam-signal-shadow validate-ws-primary CONFIG REFERENCE MAPPING SEED_CALENDAR SEED MODEL_DAY RAW_CALENDAR PLAN
finam-signal-shadow validate-ws-primary-restart CONFIG REFERENCE MAPPING SEED_CALENDAR SEED MODEL_DAY RAW_CALENDAR PLAN PREFIX

finam-signal-shadow observe-ws-primary CONFIG REFERENCE MAPPING SEED_CALENDAR SEED MODEL_DAY RAW_CALENDAR PLAN READ_ONLY_SECRET_FILE NEW_OUTPUT_DIRECTORY
finam-signal-shadow observe-ws-primary-restart CONFIG REFERENCE MAPPING SEED_CALENDAR SEED MODEL_DAY RAW_CALENDAR PLAN PREFIX READ_ONLY_SECRET_FILE NEW_OUTPUT_DIRECTORY

finam-signal-shadow verify-ws-primary CONFIG EVIDENCE_BINDING JOURNAL COMPLETION
```

`validate-*` — только file-only preparation: credential не читается, auth/client
не создаются, output и сеть не открываются. Exit0 означает успешную проверку
inputs, а не разрешение старта: `entry_window_open_now` может быть false.
Нет clock override для observe; actual entry и physical SUBSCRIBE повторно
проверяют текущие время/window/freshness. Имена файлов задаёт оператор; globs,
`.env` discovery и поиск другого account/endpoint отсутствуют.

CONFIG — только envelope v2 с `ws_primary_v1` и exact policy R0. REFERENCE,
MAPPING, SEED_CALENDAR и SEED сохраняют прежние hashes и четырёхсессионный warmup.
SEED_CALENDAR — прежний inventory warmup; MODEL_DAY — независимо заданные101
model labels наблюдаемой сессии. RAW_CALENDAR — отдельные неизменённые FINAM
schedule bodies с реальными request/receive times. Это разные назначения,
а не три имени для одного календаря.

PLAN — `primary::entry::Plan`, все поля обязательны:

```json
{
  "schema_version": 1,
  "config_sha256": "<v2 config fingerprint>",
  "account_id": "<exact selected account, local input only>",
  "read_only_credentials": true,
  "auth_policy": "provider_expiry_readonly_rtsx_v1",
  "calendar": {
    "schema_version": 1,
    "config_sha256": "<same v2 config fingerprint>",
    "model_day_sha256": "<exact MODEL_DAY bytes>",
    "raw_calendar_sha256": "<exact RAW_CALENDAR bytes>",
    "bootstrap_reference_start_ms": 0,
    "next_label_ms": 0,
    "start_not_before_ms": 0,
    "start_before_ms": 0,
    "eod_check_at_ms": 0
  },
  "startup": {"kind": "fresh_session"}
}
```

Нули и angle-bracket значения выше — только обозначения полей, не рабочие inputs.
В production фиксируются конкретные hashes/UTC milliseconds принятого окна.
Fresh требует `previous_segment_id=null`, next label=model open. Нельзя
открыть fresh посреди дня или передать prefix, который команда молча проигнорирует.
При restart `startup={"kind":"committed_prefix","binding":...}` содержит
полный `primary::restart::RestartBinding`, включая original journal hash,
previous segment/config, independent full-inventory hash и committed count.
Команда `-restart` обязательна, PREFIX обязателен; частичный хвост — отказ.
Original PREFIX остаётся отдельным неизменным артефактом lineage: его следует
сохранять и передавать вместе с successor, не считать successor доказательством
старой WS delivery. Account ID и credential path в public receipt не сохраняются.

READ_ONLY_SECRET_FILE — защищённый файл по прежнему permission contract.
Его отсутствие/права/пустое значение — отказ без env fallback. Account в PLAN,
AuthLifecycle и fixed-origin GET-only client обязан совпадать до создания output.
Само `read_only_credentials=true` не доказывает полномочия: реальные grants и
permissions по-прежнему проверяет существующий auth-компонент перед командами.
Новых POST/DELETE/order/Redis путей нет; QUOTES не подключаются.

## Output и завершение

NEW_OUTPUT_DIRECTORY создаётся однократно (Unix0700), только под доверенным
parent. Уже существующий файл/каталог/leaf symlink — отказ. Это не sandbox для
недоверенных parent directories. Все новые файлы0600/create_new, без truncate.
До transport создаются `entry.json` и новый `journal.jsonl`; restart replay
сохраняет отдельную историческую provenance и не импортирует DATA readiness.

SIGINT и SIGTERM ставят общий one-way STOP. Runner future не отбрасывается:
ожидаются bounded drain и export. Здесь нет process supervisor/restart loop.
Native watchdog/cgroup witness необходим отдельно, поскольку Rust не может
forcibly preempt зависший синхронный callback/fsync.

После подтверждённого join формируются:

- `entry.json` — config/plan/calendar pins, startup/frontier, без raw account;
- `journal.jsonl` — закрытый source/model/feedback/commit/terminal журнал;
- `run-report.json` — typed runner result, worker/recovery/HTTP facts;
- `completion.json` — отдельный post-join semantic DTO;
- `evidence-binding.json` — exact config/calendar/journal/completion pins;
- `verification.json` — повторная file-only проверка и конкретный refusal;
- `manifest.json` — hashes перечисленных файлов, пишется последним, затем fsync
  output directory. Это локальная целостность, не provider authentication.

Штатный operator STOP может дать завершённый диагностический export, но не EOD.
Если join не подтверждён либо runner не вернул drain facts, создаются лишь
`run-report.json` и `export-incomplete.json` рядом с уже существующими inputs
entry/journal: `export_complete=false`, без completion pin/финального manifest.
Потенциально изменяющийся журнал не объявляется immutable. При ошибке записи
сохраняется partial output, повтор в ту же папку запрещён.

При IO/export refusal выполняется best-effort отзыв только собственного
`manifest.json`, directory fsync и независимая запись `export-failure.json`.
Отказ unlink/receipt/fsync не пропускает остальные действия и не меняет Err
на PASS. Foreign файлы/ссылки, journal и borrowed READ_ONLY_SECRET_FILE не
удаляются. Public failure receipt содержит только фиксированные file/phase/action,
не raw path/OS error/account/credential; сам receipt тоже не гарантирован при IO
отказе. Manifest может остаться при неуспешном unlink: его наличие без successful
export return/exit не означает приёмку. Он явно содержит
`requires_successful_export_return=true`; неизвестный outcome остаётся UNKNOWN.
Повторный finish после уже успешной попытки не переписывает и не отзывает её.

Observe exit0 возможен только при `semantic_eod_passed=true` после проверки
закрытого журнала и согласованного RunReport. Exit2 — завершившаяся попытка без
semantic EOD; exit1 — argument/admission/IO/export failure. Все network/cgroup/
full-session/parity qualification flags остаются false. Только количество101
или producer PASS boolean не дают EOD.

`verify-ws-primary` — read-only, сверяет внешне закреплённый EvidenceBinding,
числа/provenance/order/commit/fault/drain и печатает semantic результат. Передача
самосогласованных pins не удостоверяет FINAM и не заменяет SHA immutable package
или native cgroup evidence. Для review сохраняются также исходные public inputs
и predecessor journals; локальный PLAN с account передаётся только по принятому
обезличенному порядку, credential никогда не входит в пакет.

## Остающиеся ограничения candidate

Первый bucket без committed overlap не имеет proof-less recovery: нельзя
очистить canonical latch одним fresh DATA. B7 добавил STARTUP_ANCHOR только
когда ни одного model M10 ещё не committed/missed и cutover равен model open.
Нужен новый nonempty closed-history GET по independently pinned диапазону
calendar reference start → model open, с прежними5s/depth/size/identity checks
и сравнением уже известных M1. Это ноль model callbacks, не восстановление
старой WS continuity и не доказательство отсутствия сделок в пропущенных минутах.
Новый physical owner/bootstrap/admitted fresh DATA обязательны отдельно.
Первый partial M10 позже всё равно требует своевременного REST_EXCEPTION.
Обычный gap сохраняет прежние overlap2/missing≤6h/однократный historical replay.
Verifier связывает source validation → retained raw → canonical resolution →
episode ready; одной producer scope/Ready boolean недостаточно.
Проверка signal→native exit/cgroup и полный packed-source replay также остаются
частью финальной квалификации. Этот документ не разрешает operational запуск.
