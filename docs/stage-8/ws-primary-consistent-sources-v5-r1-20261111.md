# V5 R1 — reference installation и retained witness identity

Статус: source correction для повторного review, не независимая приёмка.
Base R0/HOLD: `f70e592fc9fe35cf879becbddbaf56caa501997a`.
Заключение: `FINAM_CONSISTENT_SOURCES_V5_SOURCE_HOLD_f70e592fc9fe_20261111_R0_RU.txt`.
Исправляются только два producer/reader контракта V5-1 и V5-2. Название policy
остаётся `ObservedBoConsistentSourcesV5`: это correction ещё не принятого V5,
не новые market-data разрешения. Документ R0 остаётся историческим; ниже уточнён
его контракт reference-map. Принятый V4 и первоначальный HOLD не переписываются.

## V5-1: факт установки отделён от успешного resume

После успешного `add_reference` producer выводит `primary_reference_installed`:
config/generation, role (`Bootstrap` / `StartupAnchor`), plan SHA, admitted source
SHA, исходный range и фактическое время фиксации события. Диагностика перехода
старой reference к новой выводится перед этим фактом. После него reader меняет
свою comparative reference-map. Если запись evidence не удалась, продолжать
ту же цепочку нельзя.

Bootstrap resume идёт после установки. `primary_bootstrap_proof_applied` всё ещё
значит успешное продолжение WS frame; оно не подменяется installation event.
При безопасном stale mixed-frame отказе reference A уже установлена и остаётся
известной обеим сторонам, даже без applied event. Следующая startup-anchor B
может иметь ту же inventory и пересмотренную цену; diagnostics A→B проверяются
по прежней карте, затем installation B обновляет её.

Reader требует source snapshot и связанную installation evidence текущей
generation/config/range. Bootstrap дополнительно связан с предшествовавшим
request/plan; startup-anchor — с соответствующим последующим validated plan.
Неподтверждённый anchor к EOD отвергается. Простой `source_snapshot`, в том числе
optional Audit, не устанавливает reference. Missing/extra inventory остаётся
строгим отказом. Эта правка не меняет legacy pre-IO clock sample в bootstrap,
не вводит новый market-data gate и не расширяет deadline.

## V5-2: одна стабильная identity при dedup одинаковых значений

Retained table по-прежнему хранит первую реально сохранённую версию для exact
OHLCV данного minute slot. Если такая версия есть, `rest_proof_ws_witness`
выбирает её прежде текущего cache. Поэтому A(w1)→B→A(w2) связывается с REST B
через тот же w1, который останется после recovery cache eviction и будет
указан при последующем discard A. w1 не выдумывается: его исходные RX/change
timestamps и frame hash не меняются. w2 остаётся реальным cache observation,
но не переопределяет identity уже deduplicated retained entry.

Не добавляются все дубликаты/witnesses в бесконечный журнал. Прежний лимит
retained table 4096 и exact witness/source сравнение reader сохранены.
Unknown postcommit price не объявляется known repeat; discard не меняет
canonical/source/model/позицию/PnL, cache и fresh DATA/readiness.

## Воспроизведение и regression

Перед production correction два новых managed Rust-сценария были выполнены на
неизменённых production bytes R0. Оба дошли до runtime EOD: по 101 commits,
один recovery, model prepared. В обоих exporter получил независимый
`VerifyError::Source`. В handoff приложены ожидаемый exit 101, raw Rust log,
результат и test-only patch к R0. Это actual Rust reproduction, не Python-проекция.

После correction те же сценарии проходят полный runtime → exporter → независимый
verifier, а их raw комплекты проверяются также отдельным file-only CLI того же
source. Используется существующий historical synthetic calendar, без переноса
времён реальных наблюдений и без provider/VPS вызовов.

1. Stale mixed bootstrap: A установлена, исходный RX сохранён, старый frame
   не вызывает модель; generation 2/fresh DATA, startup-anchor B с новой ценой,
   полный день/EOD. Два installation facts, но нет ложного applied bootstrap.
   Отдельные контрпримеры: missing A/B installation, wrong source/plan,
   snapshot-only; missing/extra B inventory. Для inventory negatives заново
   создаётся допустимый REST capability, пересчитываются source/transport ссылки
   и outer seals; исходные canonical/model/commit payloads остаются неизменными.
2. A(w1,+100)→quarantine B(+200)→A(w2,+300)→recovery REST B→eviction→repeat A.
   Ровно один replay callback для bucket; before/after discard исходная
   source-step-commit тройка одинакова и readiness stamp не обновляется.
   Проверено буквальное равенство witnesses initial REST link/discard и w1.
   Контрпримеры: witness w2 вместо w1, чужой canonical source hash, неизвестная
   postcommit цена. Runtime-отказ unknown price также сохраняет прежний V5 test.

Новых test methods — 2, десять новых адресных negative subcases внутри них.
Предыдущие 14 V5 methods и legacy regressions сохраняются. В пакете пять raw
positive EOD-комплектов: прежние три регенерированы этим source, плюс два R1.
Targeted/full test counts повторно не суммируются. Hash-link/semantic tests не
объявляются аутентификацией каждого WS packet или защитой от согласованной
подделки всего source/evidence. Retained table через process restart не переносится.

## Следующий gate

Пока только source review. Cargo/lock, model/feedback, WS-first, live/grace/ingress
budgets, retry limits и стратегия не меняются. VPS/ELF/inputs/timers не трогаются;
FINAM orders/POST/DELETE, DB0 и runtime-live закрыты. Source correction сама по
себе не даёт clean-session credit, ALOR parity или live micro admission.

После acceptance остаются T7/T8: точные Linux runtime + совместимый Rust reader,
native BO/EOD/drain и новый согласованный execution/materializer/classifier.
R0 ELF/reader нельзя выдавать за эту R1 поставку; новый запуск отдельно разрешается.
