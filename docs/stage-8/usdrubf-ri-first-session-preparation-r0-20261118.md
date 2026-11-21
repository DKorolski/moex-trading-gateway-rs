# USDRUBF / RI: подготовка первого модельного контекста, R0

Baseline: `1b0104f90e96f42090148bf5b453a12d2bad80f5` (IMOEXF model entry R2).
Статус: source preparation; независимый SOURCE ACCEPT ещё не получен.
Этот срез не добавляет operational entry, transport, release или права запуска.
VPS, действующая сессия и таймеры IMOEXF 19 ноября не меняются.

## Завершённый scope этого среза

Добавлен явный `initial_context=usd_ri_first_session` для локального Manifest.
Он подготавливает только четыре ветки: USDRUBF `weekday_only` /
`weekend_context_no_trades`, RIZ6 `runtime_accepted_only` /
`ri_confirmed_closing_context_v1`. IMOEXF seed остаётся пустым: существующий
IMOEXF deployment не переносится в новую generation и не меняется неявно.
Обычный ingest и `imoexf_first_session` сохраняют прежнюю семантику.

Новый режим требует непустую историю обоих инструментов и exact accepted
mapping `6a508df3`: USDRUBF@RTSX/tick0.01/lot1, RIZ6@RTSX/tick10/lot1,
catalog SHA `9e06d4ff4c66a9f88534bd4f46404b12588a1c716f1c64373a97174401376b0c`.
Это привязка к принятому историческому snapshot, НЕ новый FINAM catalog GET.
Перед operational запуском live metadata/account/calendar проверяются заново.
RI — только RIZ6, expiry 17.12.2026; никакого автоматического rollover.

USDRUBF использует существующий явный bootstrap reaction-context. Для RI
добавлен отдельный одноразовый import в пустой контекст до будущего cutoff:
исходные receipt/durable clocks не переносятся назад, источник не становится
LIVE от старости или от факта bootstrap. Нельзя повторно импортировать после
freeze или использовать import как постоянное исключение для поздних данных.
RI сохраняет собственный completeness gate: 92 бара, начало не позднее07:10,
последний бар не ранее23:30, две готовые prior anchors. Не навязывается IMOEXF
101-bar grid. Preparation ограничена 1000 входами/14 днями/2..6 готовыми
regular anchors; это не автоматическая ротация контекста.

RI checkpoint хранит исходный bootstrap prefix. Restore заново проверяет import,
сверяет receipt journal и canonical prefix, затем прежние frozen snapshots.
Подмена import receipt с пересчитанным checksum без соответствующего prefix
отвергается. Обычные последующие входы по-прежнему проходят ordinary cutoff.
Отказы bootstrap атомарны: исходное состояние не меняется.

## Что не изменено и что меняет fingerprint

Числовые ядра `imoexf.rs`, `usdrubf.rs`, `ri_engine.rs`, selected profiles,
fees, virtual quantity и торговые правила не менялись. Внешние ALOR fixtures
не редактировались; numerical parity проверяется прежним независимым oracle.
Но изменены context/init source bytes, поэтому aggregate implementation hash
и RI config identity должны измениться. Совместимость checkpoint между старой
и новой сборками НЕ заявляется; миграция/продолжение старой generation закрыты.
Старые установленные ELF не заменяются и не переименовываются в новую сборку.

SQL input -> два outcomes -> checkpoint/commit/reopen/duplicate используют
прежний Journal. Нового persistence/recovery framework не добавлено.
Все новые результаты остаются `declared_clock_replay`,
`order_emission_allowed=false`, `observed_clean_session=false`.
Полноценные observed-сессии, EOD/ALOR live parity этим не подтверждаются.

## Проверки

16 новых unit tests: четыре READY arms, отсутствие неявного init IMOEXF,
ordinary late-ingest без ослабления, exact mapping, missing/short context,
граница cutoff, foreign/grid/sequence/nonfinite inputs, 91/92 RI boundary,
atomic errors, bootstrap replay/checksum, SQL reopen/duplicates/generation,
а также synthetic virtual actions обоих инструментов.

В отдельном test-only fixture USDRUBF выполняет виртуальную MR-сделку,
RI — BO pending/entry. Это SYNTHETIC данные октября, НЕ FINAM сделки
18/19 ноября. 26 входов дают52 outcomes четырёх веток; два остальных IMOEXF
arms остаются без новой истории и модельных шагов. CLI replay/export,
побайтно сохранённые input clocks и запрещённые mutation cases проверяются
дополнительно. Нулевой position после плохого bootstrap не считается успехом.

```sh
cargo test --offline --locked --manifest-path crates/finam-shadow-models/Cargo.toml
cargo clippy --offline --locked --manifest-path crates/finam-shadow-models/Cargo.toml --all-targets -- -D warnings
cargo test --offline --locked -p finam-signal-shadow --features models-bridge
cargo clippy --offline --locked -p finam-signal-shadow --features models-bridge --all-targets -- -D warnings
```

ALOR comparison: существующий `scripts/finam_shadow_models_parity.py`, все три
модели, exact handoff13 ноября. Он отдельно проверяет normalized states,
официальный comparator, negative cases и модельную экономику.
Новые tests не считаются дополнительными независимыми наблюдаемыми сессиями.

## Остаток до запуска — без новых архитектурных подэтапов

1. Native admitted producer для USDRUBF/RIZ6: собственные mapping/calendar,
   warmup inventory и actual source clocks, повторное использование принятого
   auth/WS/REST/recovery. Не копировать IMOEXF schema с заменой строк:
   current `WsPrimaryConfig`, wire, source admission, HTTP/WS subscription и
   bridge/reader ограничены IMOEXF и должны получить согласованный новый путь.
2. Завершить source tests пути admitted M10 -> четыре model outcomes -> SQL
   checked readback -> source advance last, bounded STOP и matching reader.
3. Linux release qualification, один execution package для четырёх новых
   веток и отдельно принятый/разрешённый bounded readonly запуск.

Окно USD нельзя слепо скопировать из IMOEXF: BO wait4 даёт первый signal11:00,
logical action11:10, а доказуемый next-open виден после закрытия11:20.
Для проверки именно USD BO нужен bounded deadline после этой границы.
Текущее IMOEXF окно до10:22 не продлевается этой подготовкой.

В этом R0 source review достаточно проверить только новую инициализацию,
clocks, atomicity и восстановление. Не объявлять существующие незавершённые
USD/RI producers уже готовыми по результатам локального JSON replay.
20 observed sessions и ежедневный scheduler для них ещё не активируются.
FINAM order endpoints, broker dispatch, Redis DB0 и live micro закрыты.
