# WS-primary: bounded acceptance → reader delta → longer model-shadow

Дата: 10.11.2026. Время — МСК. Этот документ не разрешает operational action.

## Принятый baseline

Run `ws-primary-nonblocking-rest-20261110-0700-r1`: **BOUNDED RESULT ACCEPT**.
Publication commit: `3671bae9d683e5ade7c8e691274b13c7b4a57da2`;
Git TREE: `600e385ba68eae6edaeb8b5aa3fee734e695f751`.

Original sealed result SHA-256:
`79e1e7b3961e628077b59e1c29205f8706401d50a0dd174b94e5c0c6bfc7e601`.
Reviewed result package SHA-256:
`23f41ed9b74bbd8b34afe064bbe7ea0009ac53261b63f71d28b86007616c6ae2`.
Review: `FINAM_BOUNDED_RESULT_REVIEW_20261110_R0_RU.txt`.

Шесть меток 07:00…07:50 приняты: WS_LIVE=2, REST_EXCEPTION=4, recovery=0;
commit latency + clock ≤1581 мс при бюджете 2000 мс. REST authorization
обновлялась; WS reconnect с новым grant не наблюдался. STOP квалифицирован по
durable exit=2 и process/cgroup witnesses. Все решения — before_wait/Flat.
Full session/EOD/ALOR parity/virtual fills/live micro не квалифицированы;
clean full sessions added=0. Прежний run_id потреблён и не повторяется.

## Узкая source/reader delta

1. Rust `primary/verify.rs`: COMMITTED_MODEL_INPUT для Proof проверяется по
   actual_m1 конкретного canonical input той же метки, причём его commit должен
   предшествовать assessment в journal. Будущий input в полном scan не может
   подтвердить прежнее утверждение. Самосогласованного forged expected_m1
   больше недостаточно. Provisional assessment не превращается в committed
   evidence. Это file-only verifier, не runtime admission/REST-veto.
2. `scripts/ws_primary_rest_diagnostics.py`: V3 comparison kind читается из
   assessment.kind (object/list), отдельно от event/role/status/basis.
   Proof/Audit считаются отдельно; legacy top-level Match/VolumeRevision
   используется только для legacy audit без V3 assessment. Unavailable,
   unassessed, unknown и missing outcomes не становятся Match.
3. Для committed diagnostic attestation связываются preceding
   source → step → commit той же метки и hashes; expected M1 сравниваются с
   actual canonical M1. Ошибка снимает диагностическое утверждение, но не
   меняет сохранённый runtime verdict. Этот вспомогательный reader не заменяет
   полную canonical admission/hash verification штатного journal verifier.
4. Реальный fixture: Proof Match=1, Proof VolumeRevision=3, Audit Match=1;
   volume_revision_proofs=3, volume_revision_audits=0,
   volume_revision_observed=true. Старый result.json с false не переписывается:
   обновлённая интерпретация хранится отдельно вместе с original hashes.

Comparison counts — учёт заявленных kinds, не общий parity PASS. Отдельные
integrity/basis flags обязательны для автоматического использования.
Provisional wire provenance не аттестуется без raw WS timeline. Даже при
корректном committed basis model impact остаётся
NOT_EVALUATED_REQUIRES_OFFLINE_REPLAY.

Стратегия, BO wait, feedback, candle-fill, data admission, source selection,
nonblocking REST, deadlines, HTTP/WS owner, auth и Cargo не меняются.
Установленный ELF и прежние immutable packages не меняются.

## Точность protected-state формулировок

Доказано совпадение пяти выбранных unit states, 147 file hashes и activation
marker с accepted snapshot. Содержимое DB0 и весь VPS не сравнивались.
Корректный вывод: проверенные protected surfaces не изменились; DB0/order
no-touch следует из принятой read-only процедуры и закрытых путей исполнения.
Дополнительный доступ к DB0 ради данного review не нужен и не выполняется.

## Следующий достаточный operational slice — пока только подготовка

После source review reader delta подготовить отдельный exact execution-пакет
на следующую согласованную fresh session. Дата/календарь/inputs/hashes/ELF,
таймеры, STOP/deadline и новое одноразовое run_id должны быть закреплены
до установки; автоматического rollover прежней попытки нет.

Reader-only правка сама по себе не требует пересборки и переустановки runtime
на VPS. В execution-пакете можно сохранить уже принятый runtime ELF, отдельно
закрепив версию нового offline reader для послепрогонной проверки. Идентичности
runtime source и reader source при этом показываются раздельно; старый
встроенный diagnostic counter не становится исправленным задним числом.
Если решим устанавливать новый ELF, для него потребуется обычный release
acceptance. Ни один из вариантов не включается данным документом автоматически.

Цель — обычный model-shadow день, который охватит:

- seed и fresh start, затем реальные закрытые M10;
- метку 10:00, обрабатываемую после close 10:10: BO wait_hours=3.0 считается
  от метки 07:00. Открытие time gate не гарантирует signal/fill;
- по возможности календарный EOD: nominal последний M10 label 23:40,
  close 23:50, production EOD check 23:50:09.900; фактические значения заново
  проверить по принятому календарю и inputs;
- естественный EndOfSession и штатный file-only EOD verifier, отдельно от
  аварийного/планового containment. RequestedStop=2 не выдавать за EOD PASS.

Достаточная диагностика: origin/reason каждого M10, имеющиеся
boundary receive/ready/model/commit timestamps, причины REST exceptions,
auth generations/refresh failures, Proof/Audit assessments и их basis,
intents/fills, durable exit и worker/process witnesses. Raw WS packet capture
или фиксированная доля WS не становятся новым admission gate. Дополнительное
инструментирование boundary, если действительно нужно для расследования,
оформляется отдельно и не включается скрыто в эту reader delta.

Не требовать искусственного сигнала, reconnect, mismatch или fill ради PASS.
Ненаблюдавшиеся сценарии отмечать как непроверенные. Полные clean sessions
засчитываются только после отдельной приёмки согласованного full-session
scope, не по короткому smoke и не только по удачному транспорту.

Для текущей ALOR parity нужен export за то же окно: M10, exact profile/seed,
model state/decisions и согласованные tolerances. Freeze 27.09 — historical
reference. Отсутствие актуального ALOR export не мешает готовить и отдельно
согласовывать FINAM-only observation, но не доказывает ALOR parity.

Этим документом НЕ разрешены установка, arming, FINAM requests, новый run,
operational DB0, broker dispatch/POST/DELETE, реальные ордера и live micro.
Следующий run — только после отдельного execution acceptance и разрешения
оператора. Новый CI/PR цикл не является частью reader fix сам по себе;
синхронизация main выполняется на логичной принятой границе.
