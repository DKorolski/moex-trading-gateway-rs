# WS-primary resilience R1: RES1/P2 и N1/P3

Статус: SOURCE REVIEW CANDIDATE, 05.11.2026. Предыдущий reviewed tree:
`f5f9547a3ffdeb332954620c4264874c1c92fcfb` (SOURCE HOLD, не accepted baseline).
Accepted baseline `6467abdbffb887a403e62ee3065961fc69132b96` сохраняется.
Новый exact tree и inventory фиксируются manifest/safety пакета после gates.

## RES1/P2 — устранение непоследовательного delayed bootstrap

`resume_bootstrap_frame` после max_ingress_age_ms проверяет полностью исторический
Missing frame через ту же policy-bound `compatible`, что последующий ingest.
ObservedBo1sVolumeAuditV1 допускает только volume-only при одинаковых OHLC,
instrument, timeframe и open/close timestamps. StrictBoundaryV1 требует прежнего
точного совпадения, включая volume.

Ничего не ретаймится: frame hash/received_at остаются первоначальными. Все Missing
строки должны быть закрыты до **исходного** receive и иметь admitted reference.
Generation/physical owner, fresh_seen, row/byte/cache/queue budgets сохранены.
Delayed mixed/live frame не получает исключения; ingest остаётся атомарным.
После успешного отбрасывания истории cached_m1=0, last_live_data=None,
subscription_data_ready=false; model/callback/feedback не вызываются. Только
следующая отдельная fresh строка устанавливает data readiness.

Это существующий bootstrap/reference budget до 5 секунд. Он НЕ изменяет новый
M10 proof/callback budget 2 секунды, proof schedule C+1s или другие latency fences.
Auth, стратегия, fingerprints/policy/reader, reconnect, units/ELF не изменялись.

Регрессии используют настоящий BootstrapBridge и source-admitted snapshot,
production PrimaryProcessor::from_config и привязанный physical subscription owner.
Проверены WS/REST volume 1548/1543 и 1543/1548 при возрасте frame 1999/2000/2001/
3000 ms, включая T+100 -> T+3100, audit VOLUME_ONLY и исходные hash/clocks.
Негативы: strict mismatch, четыре OHLC поля, identity, отсутствующая reference M1,
mixed/live tail за ingress boundary, чужие owner/generation, превышение bootstrap
deadline. Вовремя обработанный mixed frame сохраняет реальный старый receive,
а не получает время завершения HTTP.

## N1/P3 — точная причина recovery overlap failure

При несовместимом overlap выбирается фактически несовместимая пара rows, а не
первые строки bucket. При разной длине inventory выбирается отсутствующая или
лишняя minute. Отказ CanonicalConflict и неизменяемость committed prefix сохранены.
Тесты: первая строка совпадает либо отличается только volume, а ценовой конфликт
находится в девятой minute; отдельно отсутствует девятая reference minute.
Проверяются timestamp, ожидаемые/наблюдаемые значения, source hash и отсутствие
продвижения уже committed frontier.

## Red -> green и границы

До исправления новые регрессии запущены на production-файлах, побайтно равных
reviewed R0. RES1 воспроизвёл Source(Continuity) при age=3000 ms; оба N1 теста
показали отсутствие точной диагностической записи. Их ожидаемые failed logs
сохранены отдельно от окончательных успешных gates, с hashes production-файлов.
Это локальный Rust regression, НЕ replay первоначального FINAM инцидента.

SOURCE HOLD закрывает reviewer после проверки R1; разработчик не назначает ACCEPT.
Новых FINAM запросов, smoke, SSH/VPS/Docker/systemd, credentials, установки,
активации, Redis/DB0, Git/main/origin или реальных сделок в этой итерации нет.
Durable systemd exit witness остаётся задачей следующего release/execution.
Ранее раскрытые operational EOD/ALOR parity/live-micro ограничения сохраняются.
