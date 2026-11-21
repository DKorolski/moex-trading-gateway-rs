# RI reviewer differential — постоянная локальная регрессия

Numerical source трёх моделей/шести policy arms принят на
`017698205c482087334ac6c4d13fd9b65a19422d`. Descriptor приёмки:
`shadow-models-ri-source-acceptance-20261113.json`. Corrective R1 не требуется.
Review ZIP SHA-256:
`b97cec104f383904b06e727a126774c01fd0720de587a80c9dcedc0b5644e5f1`.
Локально проверены CRC, safe paths/types, все62 checksum records и совпадение
вложенного письма с отдельно полученным. Это не новый review numerical code.

## Что сохранено

`scripts/reviewer/ri_targeted_differential_v1.py` — точная неизменённая копия
переносимого reviewer runner. Соседний provenance JSON закрепляет исходный
member, source-review hash, hash generator и exact generated input SHA-256:
`548a2e6484e1354e193b5bba97c2e1476945a8cb11b1258d4b90f22c513a4e21`.
Тяжёлые expected/actual datasets не дублируются в Git: генератор воспроизводит
входы, независимый pinned upstream oracle — expected. Binary получает только input.

Обёртка `scripts/finam_shadow_ri_reviewer_regression.py` проверяет целостность
handoff/generator, запускает original runner, проверяет exact input identity,
coverage/build hash и дополнительно вызывает независимый mark audit из общего
checker. Запись только в новый output directory, без изменения source/handoff.

```sh
python3 -B scripts/finam_shadow_ri_reviewer_regression.py --binary /ABS/TARGET/release/finam-shadow-ri-fixture --handoff /ABS/finam-shadow-models-2026-11-13 --output /ABS/NEW_REGRESSION_REPORT
```

Набор:12 input cases,24 case/policy replays,2304 state rows. Полные синтетические
сетки101/89, MR shared quota, long reentries/emergency/hour/EOD, mirrored short,
Friday, сезонные границы, переход13/14 июля, return guards, sixth-bar extreme.
Нельзя утверждать, что mirrored short case содержит три BO entries: он содержит
один; three_entries_short проверяется отдельным golden fixture из принятого набора.

Локальное воспроизведение на принятом RI binary: PASS, в том числе80 model-point
exit mark checks этого дополнительного набора. Это отдельные80 synthetic checks,
не дополнение к счётчику47 исходного RI golden и не утверждение о FINAM fills.
Input/expected/actual, branch coverage, logs, summary/manifest сохраняются в output.
Статическое поле original runner `source_candidate=01769820` — reference origin,
не аттестация произвольного future binary. Wrapper отдельно пишет actual binary SHA.

## Границы и следующий шаг

Это opt-in local regression command, не новая обязательная root CI/job и не
operational gate. Numerical Rust/Cargo не менялись; новый полный Rust build/CI
для импорта reviewer tests не требуется. Принятые49 debug/release unit tests
остаются самостоятельной ранее выполненной квалификацией source01769820.

Runner проверяет полную synthetic grid и конечный flat, но **не вызывает**
`RiSession.complete()` и не доказывает observed EOD/readiness/clean session.
Новые clean sessions=0; release/execution/deployment authority не появляется.
VPS, V6, broker transport/dispatch, account/credentials и main/origin не менялись.

Следующий общий source slice: FINAM M10 contract/equivalence и symbol/calendar
binding, затем шесть независимых persistent states, consumed input/mark/checkpoint,
dedup/crash/restart/fanout. Принятая WS/REST policy не меняется; диагностический
REST не возвращается в блокирующий admission. Отдельные Linux release и
операционная активация остаются после integration acceptance.
