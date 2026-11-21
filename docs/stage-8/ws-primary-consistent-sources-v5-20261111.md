# WS-primary V5: consistent observed sources — source review

Дата: 11.11.2026. Статус: новая source delta, не независимая приёмка и не deployment.
Parent: `a5de98a278e9705eca9b9eecfb566eb255c13d4e` (принятый V4).
Основание: `FINAM_V4_REMAINING_HOLD_AUDIT_20261111_RU.txt`, H1–H6.
Новый явный opt-in: `ObservedBoConsistentSourcesV5`. Старые policy и исходный
HOLD 11 ноября не переименовываются. Model/feedback, BO-only/no-riskgate,
параметры стратегии, auth, Cargo/lock, workflows и authority не меняются.

## Границы источников

| Состояние | V5 | Что не разрешается |
| --- | --- | --- |
| WS фактически Ready | WS остаётся первым | ждать optional REST или заменять уже выбранный вход |
| Boundary есть, grace не завершён | timely whole REST, reason `WsGracePending`, исходное время boundary | сокращать grace, выдавать ложный `BoundaryUnproven`, расширять live deadline |
| Валидная немонотонная provisional revision | карантин M1/M10 для WS, whole admitted REST, reason `ProvisionalRevision` | передать произвольную последнюю WS-версию модели без источника |
| Gap ещё не обработан | whole admitted recovery REST | называть replay своевременным live, повторять committed callback |
| Committed overlap/reference | отдельная same-slot price/volume version с evidence | менять исходный canonical source, seed, модель, позицию/PnL или minute inventory |
| Старая сохранённая WS-версия после REST | диагностировать и отбросить exact known repeat | обновлять cache/readiness, callback/source hash; игнорировать неизвестный price conflict |
| Stale ingress до операции writer | синхронный gate fault и существующий bounded recovery | повторно использовать stale frame, считать Clock recoverable или повторять uncertain mutation |

H1 deliberately узкий: запоминаются только реально встреченные superseded
WS-версии, в том числе карантинные предшественники. Таблица ограничена прежним
`max_cache_m1=4096`; превышение — прежний CacheBudget, не бесконечное хранение.
Повторы проверяются после physical owner/generation/identity/clock/OHLCV guards.
Сохранённая версия переживает transport reconnect внутри процесса. Перенос
этой новой таблицы через process restart не добавлен: неизвестная повторная
price revision остаётся отказом, а не неявным расширением H1.

Evidence `REST_PROOF`/`RECOVERY_PENDING` связывает исходную WS-версию (generation,
RX/change times, frame hash) с whole REST snapshot. Последующий
`WS_SUPERSEDED_REPEAT_DISCARDED` связывается с неизменным canonical source hash
и сохранённым witness. `RECOVERY_OVERLAP_REVISION` также содержит исходный hash.
Новый verifier проверяет эти ссылки только относительно уже committed prefix,
а не будущих входов полного журнала. Для V5 reference он отдельно проверяет
inventory ранее применённых bootstrap/startup-anchor snapshots. Оригинальные
canonical sources в restart reader по-прежнему сравниваются точно.

Изменённые reference bytes сохраняются отдельным snapshot; они обновляют только
сравнительную reference-map, не начальный seed и не обработанные модельные входы.
Recovery overlap не replay-ится. Неизвестная/пропавшая/лишняя M1 не маскируется
ценовой revision. Пустой обязательный M10, invalid OHLCV/identity/account/auth,
clock regression, evidence/fsync failure и failed/late publication остаются
блокирующими; после возможной модельной мутации callback не повторяется.

## Сроки и recovery

Grace 1000 мс, ingress 2000 мс, live budget 2000 мс вместе с uncertainty и
durable publication сохранены. Proof не начинается раньше close+1000 мс.
HTTP cap 5000 мс не продлевает live budget. Старый stale mixed bootstrap frame
не получает новое RX time: до callback его continuity refusal в V5 направляется
в тот же bounded recovery. `now < received_at` выделен как Clock/terminal.
Ошибка записи/модели/clock не попадает в этот безопасный путь.

Сохранены 3 attempts, backoff 1/2/4 с, 45 с/episode, максимум 3 episodes/run,
fresh DATA текущего owner после SUBSCRIBE <=5 с, overlap=2 M10, gap<=6 ч.
Новых retry/framework, REST Match gate, WS quota, обязательного сигнала/fill,
реального искусственного provider fault или обязательного нового soak-дня нет.

## Адресное покрытие source slice

14 новых `consistent_v5_*` test methods с parameterized cases; они входят в
общую Rust-регрессию, отдельно повторно к total не прибавляются.

- T1: сохранённая Decimal-пара WS 2450/2256 и REST 2450.5/2238; whole commit,
  exact повтор в той же/новой generation, без второго callback/readiness;
  неизвестная postcommit цена по-прежнему terminal.
- T2: boundary C+950 и C+1200, truthful grace reason, ready WS first;
  поздний REST и missing-known minute не вызывают callback.
- T3: open change/high shrink/low rise/volume+price revision; quarantine,
  whole REST normal/recovery, исходные LIVE/RECOVERY origins.
- T4: versioned reference и price-only committed overlap; missing/extra
  inventory отвергаются runtime/reader. Full EOD mutation tests подменяют
  original price/hash, superseded witness, overlap witness, boundary и
  quarantine evidence — verifier отказывает даже после reseal outer hashes.
- T5: ingress 2000/2001 и future stamp; synchronous gate dispatch; actual managed
  WS/writer mixed-bootstrap >2 с открывает bounded recovery без callback
  и без изменения original RX. Это локальные duplex peers, не FINAM.
- T6: принятый V4 full-day теперь дополнительно проверяется с audit_every=6.
  Новый V5 synthetic 101-M10 день с тем же audit_every=6: optional price/inventory
  differences, unavailable audit, старый повтор, pending grace, quarantine,
  recovery price overlap, обязательный SessionEnd и независимый EOD verifier.
  Три положительных V5 журнала могут сохраняться test-only через
  `FINAM_V5_REVIEW_EVIDENCE_DIR`; каждый проверяется file-only CLI из того же source.

Команды source проверки: `cargo fmt --all -- --check`,
`cargo test --offline --locked -p finam-signal-shadow`, strict all-targets/all-features
clippy, 20 Python diagnostics regressions, прежний file-only verifier и три
V5 file-only fixtures. Старый Python diagnostics не выдаёт EOD acceptance;
его исходник не меняется. Логи, checks-source binding и raw synthetic fixtures
передаются отдельно в review ZIP. Это не broker/native/17-hour soak или ALOR parity.

## Дальше, после source review

T7: новый точный Linux runtime и соответствующий offline Rust reader из принятого
V5 source; существующие BO feedback/EOD/drain и native qualification. Старый
V4 ELF и старый reader не объявлять совместимыми с V5. Это ещё не выполнено.

T8/E1–E3: новый execution package с ровно V5 literals в materializer, execute,
classifier и portable checker; exact runtime/reader hashes, новые inputs/run_id,
реальный status/reason classifier в выводе. Отказы для V3/V4 mismatch и старого
reader должны быть до активации. Использованный execution/result не переписывать.
Это отдельный следующий package, не скрыто выполненная часть source acceptance.

VPS/таймеры/operational inputs и установленный ELF этой source delta не менялись.
FINAM POST/DELETE, broker dispatch, orders, Redis/DB0, runtime-live и live micro
закрыты. Полный наблюдаемый день, clean-session count, ALOR parity и live admission
остаются отдельными решениями. Main merge не нужен для получения source review.
