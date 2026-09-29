# O2 non-activating installation — review 304cd56

Дата: 2026-09-29.

**Вердикт: INSTALLATION PACKAGE ACCEPT.** Блокирующих замечаний в рассмотренной границе пакета не обнаружено. Принимаются установщик, фиксированный inventory, новая installation identity, журнал и ограниченный pre-activation rollback. Разрешение на фактическую установку на VPS остаётся отдельным действием пользователя; bootstrap и activation этим review не разрешены.

| Привязка | Значение |
|---|---|
| Commit | `304cd56bd33e2145f327c5f1ea02f56837cc3e62` |
| Tree | `e7f166d83691859ed7b2b9072f2c37c6cbed1a85` |
| Parent / принятый artifact | `7196aaac7c0bf45a03d90742d8ef483078649de6` |
| Installation ZIP SHA-256 | `22c034decddbd3062a40b1d6a95b71d5beec222c2298f83f112659890ef68bbc` |
| Вложенный artifact SHA-256 | `1069cbeb597e902126ebdb0b2c01ef52dca45420dfe2a69a6963b939de33ac4a` |
| Installation ID | `stage8b-p1f-o2-baseline07-7196aaa-v1` |
| Installation identity SHA-256 | `7252b88c2c2ff36e857e26bc7513c1be3d7974709f22957ab721b123331da8d6` |

## Основания принятия

**Scope и bytes.** Пакет не пересобирает бинарники. Он использует точный принятый ZIP `7196aaa`, включая baseline07 supervisor, оба O2 facades, public templates и units. Сравнение с predecessor подтвердило отсутствие изменений Rust/Cargo, deploy/config, workflows, runtime profile и принятого custody helper. Дельта — четыре новых Python-файла, два документа пакета и обновление status/roadmap/authority.

**Installation identity.** Reviewer независимо вызвал `load_package` и пересчитал canonical inventory/hash: совпадение с package specification. Устанавливаются 16 файлов и три новых каталога. Старый compatibility manifest получает фактические новые hashes; полный O2 manifest связывает payload/custody, predecessor, installer и helper. Будущие signed O2 documents должны использовать именно новую identity, а не historical O1 hash.

**Preflight.** До payload replacement проверяются exact O1 manifest, старые managed bytes, owner/group/mode/single-link, service identity, пустые state/quarantine и отсутствие operator/authority/materialized inputs. Неожиданные новые каталоги не усыновляются автоматически. Native observer ограничен фиксированным Linux target, проверяет host key, unit inventory, stopped state/PIDs/jobs/drop-ins, cgroups и P1 processes. Ошибки command/query не превращаются в stopped proof. P0 observations сравниваются до/после; Redis installer не вызывает. DB15 emptiness остаётся обязанностью отдельного свежего read-only preflight.

**Write ordering и recovery.** Before-images и PREPARED journal сохраняются до замены payload. Каждый заменяемый файл проходит file fsync, atomic rename, parent fsync и exact reread. Compatibility manifest следует после payload, полный O2 manifest — последним. Только полная проверка нового состояния и неизменный observation дают APPLIED и `EXACT_O2_INSTALLED_NOT_ACTIVATED`. Mixed old/new допускается для продолжения только вместе с валидным retained journal/before-images. Publication новой identity не подменяет завершённость транзакции.

**Rollback.** Удаление ограничено фиксированными новыми файлами после проверки exact bytes; старые bytes восстанавливаются из проверенных before-images. Появление genesis/selector/source/durable/quarantine contents блокирует откат. Непустые каталоги не очищаются. ROLLING_BACK продолжает только rollback; ROLLED_BACK не открывает повторную установку той же транзакции. Исходный журнал и forensic history сохраняются.

Это практически достаточная узкая транзакция для текущего обновления. Защита от конкурентного привилегированного оператора явно вынесена в maintenance window; новый deployment framework не требуется.

## Что проверено и как читать evidence

Независимо выполнены:

- SHA-256, CRC и безопасный состав ZIP — PASS: 2498 members, 2489 tracked, duplicates/symlinks/unsafe paths — 0.
- Проверка raw commit, реконструкция tree, hashes retained logs/review и обоих вложенных accepted ZIP — PASS.
- Повторная проверка принятого O2 artifact через вложенный safety checker — PASS.
- Сравнение protected path inventory/bytes с `7196aaa` и пересчёт новой installation identity — PASS.
- Три проверки без filesystem fixture: systemd-property parser, native-observer response controls, service-UID/root-executable process inventory — PASS. Ответы manager и `/proc` здесь подставлены контролируемыми fixtures; обращения к VPS или рабочему systemd отсутствуют.

Retained Linux log подтверждает **15 tests PASS**, включая **12 отдельных durable-file frontiers** внутри recovery-теста. Это исключения после завершённой записи файла, а не OS SIGKILL/power-loss. Systemd manager также не запускался: `systemd_manager_tested=false` отражает реальный объём доказательства.

Reviewer попытался повторить полный набор на временных каталогах. Все 15 случаев остановились в fixture setup на `os.chown(..., 65000)` с `EINVAL`: UID/GID 65000 не поддерживаются текущей средой. Это ограничение reviewer environment, а не выявленный сбой установщика. Полный Linux набор не объявляется независимо повторённым; его результаты приняты по проверенным retained logs и source inspection. Custody-проверки ради запуска не ослаблялись.

**45/45 negative cases — унаследованный current-tree authority harness.** Их не следует обозначать как 45 новых behavioral installation mutations. Новое install/recovery/rollback поведение проверяет отдельный набор из 15 тестов. Уточнение счётчиков не является основанием для HOLD.

Остаточные неполные staging/temp files и прерывание до завершения custody могут потребовать ручной проверки. Пакет это документирует и не обещает универсальное автоматическое crash recovery.

## Следующий разрешаемый объём

После отдельного разрешения пользователя выполнить уже описанный узкий порядок:

1. Свежий read-only preflight целевого VPS: host identity, P0, DB15, P1 inactive и отсутствие нового operational state. Проверить exact package SHA и root-only custody staging.
2. Использовать этот установщик для non-activating install. Не выполнять daemon-reload, enable/start, genesis/signing или bootstrap в той же операции. При несовпадении состояния остановиться; не добавлять force/cleanup workaround.
3. Сохранить raw stdout/stderr, before/after observations, journal, manifest bytes/hash и свежий `status`. Критерий завершения — `EXACT_O2_INSTALLED_NOT_ACTIVATED` с identity `7252b88c…`, точными тремя binary hashes и неизменным P0 в согласованном scope. DB0 digest может естественно меняться работающим P0 и не должен сравниваться на полное равенство.
4. После принятия фактического installation evidence отдельно разрешить bounded O2. Signed authority должна связывать новую installation identity. Собрать fresh materialization, isolated bootstrap, реальные stopped proof, terminal receipt и durable-root/adoption evidence.

Повторный полный Rust-аудит, новый installer framework и расширение paper-программы не требуются. Сохраняется цель: несколько ограниченных paper-сессий для сравнения с ALOR-live, затем отдельно разрешённый FINAM live micro.
