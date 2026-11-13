# ALOR shadow handoff: перенос численных моделей в FINAM

Дата: 13 ноября 2026. Исходный handoff: `finam-shadow-models-2026-11-13.zip`,
SHA-256 `2d272312dec9cda3693962e93692a5e6c194d890367cb1dbee0ac527b7905f55`.
Accepted FINAM V6 runtime source: `f581552e9079b68cba5ed58c896fbf078f717aac`.
База локальной ветки с принятым offline-reader: `c37f220ad8648801ec66212a0e05737de3869128`.

## Решение и первый законченный scope

Пакет — новый baseline моделей, не разрешение менять установленный V6. Новый
IMOEXF Author41-short + Hybrid BO053 **не** равен текущему baseline07 BO-only.
В этой итерации переносится только численная IMOEXF combo: `weekday_only` и
`weekend_context_no_trades`. USDRUBF H4/W4 и RI prospective — следующие slices.
Работающие FINAM/ALOR контуры, GitHub/main, credentials и VPS не изменяются.

`crates/finam-shadow-models` — отдельный offline Rust workspace с библиотекой
и input-only CLI. В accepted FINAM workspace/runner зависимость не добавляется.
Это намеренная временная граница: численные правила проверяем до интеграции,
не меняя текущий transport/auth/close-fill или общую feature resolution serde.
Собственный Cargo.lock фиксирует offline build; `float_roundtrip` сохраняет
точные f64 через JSON/checkpoint так же, как в исходном Rust reference.
Фактически квалифицированный compiler — Rust 1.95.0, как в handoff.

Численный `imoexf.rs` взят из закреплённого reference: отличаются комментарий
и эквивалентный синтаксис Option/M10-grid. Формулы, приоритеты MR/BO, observation,
entry quotas, pending и комиссии не переписываются. Context/facade адаптированы
под локальные строгие DTO, без ALOR crates, collector и filesystem helpers.
Точные upstream hashes и перечень адаптаций: `crates/finam-shadow-models/provenance.json`.

## Контракт исполнения — не подмена текущей модели

Signal закрытого M10 label T планирует виртуальное исполнение по open T+10m.
Этот open доказуем из следующего закрытого M10 лишь при T+20m. Это модельная
историческая цена, не исполнение брокером и не обещание fill в прошлом.
Signal label, logical action и фактическая доступность — разные времена.
В numerical fixtures clock явно synthetic, не original broker receipt.

EOD signal label 23:30 исполняется по open terminal label 23:40; для полного
результата требуется закрытие terminal в 23:50 и flat/no-pending. При missing
terminal сохраняется incomplete, а не fictitious flat/next-day fill. MR имеет
priority, BO observation продолжается при MR/pending. Нет quotes или brackets.
Frozen commission включена в результаты новой модели; текущая zero-cost
close-fill политика V6 остаётся неизменной и её результат не переименовывается.

Для каждой context-политики отдельны Store/session/checkpoint. Freeze требует
точный ingress barrier и previous/previous2. Original receipt/durable clocks
не подделываются по времени исторического бара; поздняя коррекция не заменяет
использованный anchor. Restart воспроизводит контекст и весь numerical prefix;
это offline checkpoint bytes, **не** уже реализованный crash-safe VPS writer.

## Локальные проверки

Из корня этого worktree:

```sh
cargo test --offline --locked --manifest-path crates/finam-shadow-models/Cargo.toml
cargo clippy --offline --locked --manifest-path crates/finam-shadow-models/Cargo.toml --all-targets -- -D warnings
cargo build --offline --locked --release --manifest-path crates/finam-shadow-models/Cargo.toml
python3 -B scripts/finam_shadow_models_parity.py --handoff /ABS/finam-shadow-models-2026-11-13 --binary /ABS/finam-shadow-imoexf-fixture --output /ABS/NEW_REPORT
```

Проверяются 18 upstream сценариев / 1886 state rows, включая обе policies и
checkpoint/restart. Rust получает только `inputs.json`, не expected. Python
независимо пересчитывает oracle, сравнивает с pinned expected, нормализует actual
через официальный projector и запускает официальный candidate comparator.
Ещё проверяются 6 comparator mutations, 8 invalid-input запусков actual Rust CLI
и экономика 22 закрытых synthetic trades. Это не 22 сделки FINAM.

Unit-тесты отдельно проверяют next-open против close/threshold, MR ownership и
observer, независимые anchors, атомарный отказ при corrupted input, duplicate,
pending/non-flat restart, checksum/witness/identity/clock, exact barrier и terminal.
Они не заменяют operational soak и не заявляются как 76 upstream Rust tests.
CLI выводится в новый файл только после успешного полного replay; overwrite
не разрешён. Evidence имеет synthetic kind; operational_admission=false.

## Следующие работы без расширения scope

1. USDRUBF `previous_day07_h4_w4`: две policies, собственные stop/take/EOD priorities,
   tick rounding и economics. Сверка по 18 scenarios / 1886 states.
2. RI Author41/42: две RI policies, actual contract RIZ6, собственный prospective
   execution и MR shared quota. Не использовать IMO/USD reaction engine вместо RI.
   Сверка по 16 scenarios / 376 states. Contract roll — отдельная процедура.
3. FINAM-native closed-M10 ingress/fanout и persistent state шести веток. Сначала
   закрепить mapping инструментов и эквивалентность M10: handoff требует canonical
   внешние M10, текущий V6 использует принятую M1->M10 композицию. Не считать их
   автоматически одним контрактом и не менять gateway молча. Логика получения
   данных/auth/reconnect переиспользуется только в пределах проверенного контракта.
4. Source review -> Linux release qualification -> отдельное разрешение установки
   и наблюдения. Без новой торговой authority, Redis DB0 или order producer.

Межсерверный обмен журналами, live execution adapter и GO live micro не входят
в текущий handoff/итерацию. Указанные в новом handoff 20 clean sessions относятся
к программе новых моделей и требуют отдельного согласования operational scope;
они не переписывают принятый десятисессионный план текущего FINAM baseline и не
дают автоматического live promotion. Текущий V6 EOD принимается самостоятельно.

## Что этот slice не доказывает

Нет FINAM observed receipt certification, equivalence FINAM/ALOR M10, historical
cross-broker parity, установленного сервиса, production checkpoint writer,
continuous generation, Linux release или новых прав запуска. Полный governance
closure делается для согласованного интегрированного source scope, не путём
объявления этого offline crate уже принятым production-модулем.
