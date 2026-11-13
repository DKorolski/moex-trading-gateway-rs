# RI Author41/42 — локальный prospective port

Статус: source review candidate, не release/execution acceptance.
Baseline: SOURCE ACCEPT `7d573e292052df78a3969f82708190b564149104`.
Принятый reaction scope и hashes review зафиксированы в соседнем
`shadow-models-reaction-source-acceptance-20261113.json`. Corrective R1 не нужен;
текущая работа не пересматривает эту приёмку или accepted V6 runtime.

## Что добавлено

Отдельные `ri_engine.rs`, `ri_context.rs`, `ri.rs`, `ri_fixture.rs`, `ri_main.rs`
в существующем standalone offline crate. Нет зависимости от ALOR runtime,
gateway, Strategy host, Intent/OrderEvent, FINAM/Redis client или transport.
Общий reaction engine не получает RI flags; IMOEXF/USD numerical sources,
context/session и reaction CLI остаются побайтно неизменными от accepted 7d.
В root Cargo workspace и deployed V6 новый crate по-прежнему не включён.

RI context заимствован из pinned `reference_rust/strategy-runtime/src/ri_context.rs`.
Удалены file IO, ingress/runner; DTO локальные; freeze доступен in-memory.
Добавлены read-only getters original processing clock и проверка binding этого
clock перед обработкой моделью. Численное ядро RI, в отличие от двух reaction
model.rs, не byte-for-byte копия большого ALOR live adapter. Это выделенный
broker-neutral перенос prospective методов: формулы и порядок сохранены,
host/order/lifecycle callbacks заменены model marks и локальной assumed-fill
transition после обработки MR и BO. Происхождение и symbols — в provenance.json.

## Точный модельный контракт

- Selected `ri_author41_42_primary_combo_cost2`, RIZ6 / RTS-12.26, M10 BEGIN UTC,
  Москва UTC+3, prospective_shadow/micro_regular_v1, виртуально один контракт.
- MR: short .20R / take .020R / stop .75R; long .11R / take .005R / stop 1.00R;
  strict .005 < R/previous_low < .100; entries до label10:00 включительно.
  Shared max2 по обеим сторонам. Exit: touch stop -> strict close take ->
  20:00 time exit -> breakeven после >20 bars. MR mark — level/close/entry-price
  по соответствующему exit reason, **не** универсальный next-open.
- BO: шестой наблюдённый бар фиксирует .42R levels с first-high/low и sixth-close.
  Observer выполняется прежде lifecycle early return при MR/pending. First-hour
  extreme >1.5R выключает BO; Friday и 21.05..01.07 блокируют только BO.
  Hour entry: minute50 и строго после first+50min, prev high/low >1.01,
  return strict >-.025 / <.025. Reentry day-extreme включён; нет BO max2/entry17.
- BO entry и emergency/hour exit используют open следующего принятого model bar;
  EOD label23:00 — **same-bar close**, доступный после23:10. MR имеет первый слот,
  нет MR/BO overlap или same-close handoff при pending transition.
- MR cost2 =2 model points/round; BO=0 model points — frozen research economics,
  не утверждение о бесплатном FINAM исполнении. Нет искусственного RUB conversion.
  Legacy phase/reason строки `live_*` сохранены для parity; это не live mode.

## Контекст, время, вход и restart

`runtime_accepted_only` запрещает history_gap. `ri_confirmed_closing_context_v1`
разрешает history_gap только label23:40 и только как context, не вчерашнюю сделку.
Weekend anchors запрещены в обеих ветках. Completeness >=92, first<=07:10,
last>=23:30; до14.07.2026 >=80, first<=09:10. Два anchors обязательны для readiness;
если пары нет, session явно context_not_ready и без marks, а не synthetic warmup.

Exact selected `profiles/ri.json` скопирован из handoff; SHA-256
`30e74209c058e288ab171afb7d18cba4d9bb93a0e4927cf9a6062325e96fe989`.
Identity включает contract/profile/feed, policy, namespace и calendar, включая
excluded_date 04.11.2026, плюс hash profile и source bytes RI engine/session/context.
Замена algorithm/calendar/contract/generation не восстанавливает чужой checkpoint.

Context проверяет finite/valid OHLCV, закрытие bar, origin, original received/durable
clock и точный ingress barrier. Session дополнительно требует verified route,
следующий exact receipt sequence, original processed clock из ingress, неизменный
frozen view и возрастающий model label. Шаг stage-ится на clone; ошибка не выпускает
mark и не изменяет session. Duplicate не повторяет effect. Nonfinite raw payload
может иметь quarantined digest в context, но не сериализуется как JSON null в
restorable session prefix: session атомарно отклоняет такой вход.
Переполнение вычисленных levels из конечных OHLCV тоже отклоняется до commit,
не сериализуется как null. BO readiness сохраняет canonical finite H/L-ratio guard.

Checkpoint содержит initial context bytes/clock, selected identity/generation/day,
весь causally ordered input prefix с processing clock/routing, clock guard и
полный engine witness, включая first-bar/levels/pending/counters/private state.
Restore независимо реконструирует context и все шаги, проверяет witness; marks
при восстановлении не возвращаются и не публикуются. Текущее время restart —
guard, не замена original receipt. Нет disk writer/atomic publish protocol: он
относится к следующей общей persistent-интеграции.

Sparse synthetic inputs позволяют проверять отдельные prospective branches;
они не доказывают continuous source или полный день. `complete()` отдельно
требует точную полную model grid (101/89), реально обработанный terminal23:40,
flat/no-pending. Quarantined slot, missing/duplicate или короткий flat-prefix
не становятся complete. Intraday operational start всё ещё должен восстановить
весь текущий model prefix; cold-flat посреди дня этим port не разрешён.

## Воспроизведение

```sh
cargo test --offline --locked --manifest-path crates/finam-shadow-models/Cargo.toml --target-dir /ABS/TARGET
cargo test --offline --locked --release --manifest-path crates/finam-shadow-models/Cargo.toml --target-dir /ABS/TARGET
cargo build --offline --locked --release --manifest-path crates/finam-shadow-models/Cargo.toml --target-dir /ABS/TARGET
cargo clippy --offline --locked --manifest-path crates/finam-shadow-models/Cargo.toml --target-dir /ABS/TARGET --all-targets -- -D warnings
cargo fmt --manifest-path crates/finam-shadow-models/Cargo.toml -- --check
python3 -B scripts/finam_shadow_models_parity.py --model ri --handoff /ABS/finam-shadow-models-2026-11-13 --binary /ABS/TARGET/release/finam-shadow-ri-fixture --output /ABS/NEW_RI_REPORT
```

RI CLI получает только exact inputs и implementation ID. Expected/trace/Python
не вшиты в binary. 16 upstream cases /376 state rows по обеим policies;
на каждом case/policy один midpoint restart, затем сравнение последующих шагов.
Python независимо пересчитывает oracle, stored expected и официальный comparator.
Mark audit дополнительно проверяет цены/ownership/классы clocks и arithmetic
47 exit PnL в model points; это не 47 реальных сделок и не RUB fee gate.
Есть 6 comparator mutations и 8 invalid-input запусков реального RI CLI.

Существующие IMOEXF/USD checks повторяются отдельно с reaction binary на прежних
fixtures; их models/context/session не изменены. 49 unit tests в каждом из двух
build modes: прежние25 и24 RI. Число state rows всех моделей =4148, не число
различных observed FINAM candles. 32 RI midpoint restores не добавляют новые
fixture rows. Никакой full-session, operational или promotion verdict не выдаётся.

## Следующий достаточный этап

Source review RI delta. Затем один согласованный FINAM-native M10/input envelope,
explicit symbol mapping и calendar binding для трёх моделей; различие external
M10 и принятой V6 M1->M10 композиции проверить/версионировать, не замалчивать.
После этого — consumed input/mark/checkpoint и независимый persistent state
шести arms, restart/dedup/fanout, общий integration review. Только потом Linux
release, отдельно разрешённая установка и observation. VPS, real orders,
DB0, брокерские POST/DELETE, live credentials/dispatch и GO live micro закрыты.
Авто-roll и межсерверный обмен журналами — не этот source slice.
