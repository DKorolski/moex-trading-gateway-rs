# O2: локальная коррекция точности времени V4, 5 октября 2026

Статус: source-review candidate, **не acceptance, не installation и не O2 PASS**.
Исходный HEAD: `b8732cc7ddff1322fd569add0d3438844cc090a1`.
Точные source/authority commits и дерево фиксируются в `handoff-commit.txt`
нового ZIP. Соседний P0 WS hardening и scheduler — отдельные ранее существовавшие
изменения; в эту коррекцию и immutable archive не включаются.

## Что доказано

Однократный O2 5 октября остановился после пяти GET на
`canonical_validation / source_rejected`. Bootstrap runner не запускался.
Cleanup завершил фазу `FAILED / generation 1 / sequence 8`.
Операционные факты и исходные redacted records сохранены отдельно:
`reports/operations/sparse-o2-run-20261005/REPORT.md` и `evidence.json`.

Локально воспроизведён конкретный дефект production assembly/parser:

```text
receipt.received_at = 2026-10-02T05:00:03.100Z
trusted_now        = 2026-10-02T05:00:03.200Z
old captured_at    = 2026-10-02T05:00:03Z
result             = SourceRejected
```

Тест `observed_v4_assembly_preserves_subsecond_receipt_chronology` до исправления
завершился exit 101 именно на этом случае; предшествующие controls с целой
секундой и receipt из предыдущей секунды прошли. После исправления весь тест
проходит, включая разницу в 1 ns и равенство receipt/capture.

Это подтверждённый дефект кода, **не доказанный exact root cause VPS-попытки**:
отклонённый bundle там не сохранялся, а журнал не содержит точного received_at.
Ни повторный operational запуск, ни повторная загрузка истории для доказательства
этой гипотезы не выполнялись.

## Изменение контракта представления времени

Только observation metadata V4 — `captured_at_utc`,
`broker_truth.checked_at_utc` и совпадающее поле materialization evidence —
теперь сериализуются как canonical UTC RFC3339 `AutoSi`:

- без дробной части для точной секунды;
- 3, 6 или 9 знаков, если нужны ms/us/ns;
- без округления, timezone aliases, лишних нулевых разрядов или leap seconds.

V4 parser выбирает этот формат только по уже проверенному `ObservedV4` policy.
V2/V3 остаются строго seconds-only; их writer/parser не меняются. Поля M1/M10,
номинальные границы, правило sparse aggregation, модель no-riskgate, source plan
и policy identity не меняются. Полные receipt bytes, receipt hash и canonical
M10 сохраняются; исправлять receipt задним числом не разрешено.

Неравенства проверяются с исходной точностью: receipt <= capture <= trusted_now,
broker-truth <= capture, broker truth age <= 300 s, receipt age <= 900 s.
Ни секундного допуска, ни sleep до следующей секунды не добавлено. Тесты проверяют
отказы на 1 ns за границей, clock reversal, неканоническое время и старые данные.

Отдельный guardian receipt сохраняет существующую seconds-only projection
broker-truth timestamp. Это не подмена source: staged consumer сохраняет точные
байты bundle, а guardian повторно валидирует полный source и его SHA. Изменён
только тест guardian, не его production code или terminal history schema.
CLI/staged consumer и guardian materialization/replay проверены с дробным V4.

Старый V4 binary не поддерживает новые дробные observation fields. Поэтому
будущий artifact должен согласованно пересобрать materializer и runtime/guardian
consumers; заменять один ELF отдельно нельзя. Candidate authority binding
обновляется отдельным inventory-only commit; это не независимая приёмка нового
дерева. Checker, CI workflows, closed flags и accepted historical pins неизменны.

## Диагностика

Assembly self-check и retained-source readmission больше не скрывают тип ошибки
production V4 parser общим `SourceRejected`: exhaustive mapping выдаёт статический
код, например `source_invalid_history`, `source_invalid_broker_truth` или
`source_invalid_schema`. Сохраняются stage, collection progress и bounded JSON
до 4096 bytes. Account IDs, response bodies, credentials, paths и upstream error
strings не добавляются в diagnostic. Реальный parser rejection и redaction
проверены тестом, а не только прямым конструированием enum.

## Первичные локальные проверки (до immutable packaging)

Полные логи финального прогона:
`reports/operations/o2-v4-timestamp-correction-20261005/final/`.
Команды, exit codes и SHA-256 затронутых файлов: `result.json` в родительской
папке этих логов. Это evidence рабочего дерева, не immutable commit evidence.

| Локальная проверка | Результат |
| --- | --- |
| gateway materializer, включая observed assembly | 27 PASS |
| materializer CLI, retained replay, protected staged consumer | 14 PASS |
| durable first boot/source/transaction/recovery | 47 PASS |
| guardian/staged/systemd unit checks, serial | 64 PASS, 1 ignored helper |
| существующая WS/REST fixture 4 октября | 5 PASS |
| scoped strict Clippy (`finam-gateway`, `runtime-durable-service`, lib/tests/bins) | PASS |
| cargo fmt и git diff --check | PASS |

Итого 157 верхнеуровневых тестов PASS; вложенные child-test invocations не
прибавляются повторно. Ignored helper не объявляется отдельным пройденным тестом.
Добавлены пять регрессионных тестов, усилены существующие first-boot/restart,
guardian и CLI fixtures дробными timestamps. Полный workspace/Redis process
crash matrix здесь не перезапускался, GitHub CI не заявляется.

Первый Clippy выявил два `err().expect()` в новых тестах; они заменены на
`expect_err`, после чего выполнен финальный зелёный прогон. Default-feature
test builds показывают существующие feature-gated unused warnings; строгий
Clippy прошёл с имеющимся `finam-gateway/stage8b-r2a7-source-adapter` feature.

`current_tree_authority_check.py`: **FAIL production file-count drift** на
текущем незакоммиченном дереве, содержащем также отдельный P0 WS patch. Это не
зелёный полный handoff gate. Authority не перепривязывалась ради прохождения
проверки. Следующая фиксация должна явно разделить scope и пройти обычный
source/authority review перед operational artifact.

## Дальше

1. Передать узкий source-review snapshot этой коррекции с проверками;
   отдельный P0 WS patch не выдавать за установленный или принятый.
2. После acceptance собрать полный согласованный successor artifact и проверить
   installation evidence с сохранением terminal history `FAILED / 1 / 8`.
3. Только затем — отдельно разрешённая bounded O2 попытка в новом окне/phase.

На этом шаге не было SSH, FINAM запросов, установки, запуска таймера, сброса
one-use marker, push или merge. Operational Redis, FINAM order endpoints,
broker dispatch, real orders и runtime-live не открывались. O2 остаётся HOLD.

## Immutable source-review package

`scripts/make_stage8b_o2_v4_timestamp_handoff.py build OUTPUT_DIRECTORY` запускается
из чистого checkout candidate authority commit. Он повторяет scoped Rust/CLI/
guardian/first-boot tests, проверяет authority и её negative harness, сохраняет
полные логи, source manifest, raw Git commits, исходную authority source commit,
и восстанавливает оба Git tree без Git database. Generated evidence не подменяет
tracked files. `verify ARCHIVE` повторяет archive safety/tree/evidence checks.

В `handoff-evidence/o2-v4-timestamp/result.json` лежат новые результаты именно
упакованного дерева. Предыдущие 157 тестов не переименовываются в результаты
чистого снимка: отдельные 5 WS fixture tests в него не входят. Полный workspace
test gate, Linux artifact qualification и GitHub CI этим пакетом не заявляются.
Status candidate authority остаётся `independent_review_required`.

Authority closure меняет только production/control inventories. Source commit
содержит реализацию, тесты, packaging procedure и status/docs; его parent —
исходный `b8732cc`. В отдельном authority commit нет Rust/Cargo изменений.
Локальные untracked WS/scheduler файлы не удаляются и не попадают в ZIP.
