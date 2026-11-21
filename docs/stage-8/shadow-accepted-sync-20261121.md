# Принятый shadow checkpoint и синхронизация Git — 21.11.2026

Этот документ отделяет принятую реализацию, операционные разрешения и будущие
результаты. Это не новый strategy/auth/recovery design и не разрешение на live.

## Принятые исходники

- Последний SOURCE ACCEPT: `10e74757a8786ad6aa0e771ffb199cf2a0fcbb77`.
- Его Git tree: `495a91041d1eabd2506336c744d9093742d03f81`.
- Включён принятый IMOEXF precision fix `f19eb207847510a5d26df900d46c2a1e2435801c`.
- Последняя source-коррекция USD/RI: необязательный строгий boolean
  `is_data_snapshot`, исходные секундные границы Schedule, положительное
  пересечение M1 с Trading/Auction и seed до 00:00 МСК дня запуска.
  Model freeze остаётся 07:00. См.
  [source contract](usd-ri-real-input-compatibility-20261120.md) и
  [IMOEXF reader correction](imoexf-retained-rest-readback-precision-20261119.md).
- Стратегии/численные параметры, V8 WS/auth/reconnect/STOP этой синхронизацией
  не меняются. Frozen RI — RIZ6, не автоматический roll.

Exact source review-package SHA-256:
`c83fe44248141b8207144814d1885d7702362bf188df527516509bec79007769`.
Exact Linux release-package SHA-256:
`419473832bfaed12072b4c58557d9d41223f8d1984e863dcafe9b546439d71f5`.
Exact production ELF SHA-256:
`df8569b70efa346aa81b8d6835ad78322f957661e8f1044385819f99a7f3d612`.

SOURCE ACCEPT не заменяет RELEASE/EXECUTION ACCEPT. Git checkout тоже не
заменяет sealed artifact и его original receipt/pins.

## USD/RI: разрешено и настроено, но ещё не исполнено

Execution R1: `usd-ri-v8-models-20261123-0700-r1`.
EXE-CAPADD-1 закрыт: нормализация только известного `CAP_` prefix, строго два
полномочия; effective SecurityOpt также проверяется. 98 wrapper tests и 35
package negatives независимо подтверждены reviewer. Исходный R0 не применять.

Execution ZIP SHA-256:
`d66cec8f445f45506fb73f79360b5c52f3e2d1fc3560a77a2f5f5ce5300ab35f`.
Execution manifest SHA-256:
`f1b9617afc81ade1d07122c411d5e3d551abe48000339649ec2843573311e0e0`.
EXECUTION ACCEPT report SHA-256:
`de388da45c6f0cd7fbf50bcc98627f5d6d1483c19bac7b16b5cff4a0194f5650`.
Installation evidence ZIP SHA-256:
`71b5e71ffef7741686683c6ac772c1b5e773e464ab1b3aad4468ffc6066b2d6b`.

20 ноября оператор отдельно разрешил install-and-arm с уровнем безопасности
как у IMOEXF. Сверены принятый IMOEXF пакет и реальный прежний HostConfig:
bridge, private cgroup, SYS_ADMIN/NET_ADMIN, unconfined seccomp/AppArmor,
без privileged mode и host mounts, 1 CPU/1 GiB/128 PIDs. Observer UID999/GID996,
NoNewPrivileges и пустой CapabilityBoundingSet. Это ограниченное разрешение
для одного прогона, не утверждение полной безопасности root/PID1 unconfined.
Новые security/permission frameworks не добавлялись.

23 ноября, все часы МСК: история с 06:45; fresh metadata в 06:59;
старт только 07:00:05–07:02; source stop 10:22; host STOP 10:22:05;
emergency 10:24:30; hard deadline 10:25. Четыре USD/RI arms,
20 M10 на источник. Без retries/restart/catch-up и продления окна.
Timers на VPS действуют независимо от Mac; Persistent=false, пропуск не
догоняется. При нарушении обязательного инварианта — HOLD.

Последняя read-only проверка: **21.11.2026 07:42 МСК**. Пакет/units совпадают,
все три timers active/waiting; попытка ещё не начата, service journals пусты;
защищённые сервисы неизменны, NTP synchronized. Реального Monday seed,
production admission и наблюдённых WS/model/STOP результатов пока нет.
Строгая PID/monotonic snapshot policy не ослаблялась: штатный чужой restart
может дать HOLD; это известный P2, не скрытый override.

## IMOEXF: отдельный scope

Candidate `imoexf-v8-models-20261123-0700-r0` подготовлен, но его установка и
arming не выполнены. Нельзя считать его включённым из-за USD/RI authority.
Старый остановленный IMOEXF container не менялся и не переиспользуется.
Candidate ZIP SHA-256:
`2ec3a59d3b4d302192186a6dac6fbcd7b63f54e3db424eb02d3a16301e9a6e2d`.
Его отдельные execution review/операторское разрешение сохраняют силу границ.

## Что включается в Git

История принятых source-итераций до `10e74757` сохраняется; текущий `main`
подмешан history-preserving merge без изменения принятого source tree.
Незакоммиченные старые локальные broker-cli/операционные скрипты не включаются.
Фикстуры с raw OHLCV/эталонными traces остаются тестовыми входами, не live evidence.

Production inventory GOV-CI механически связывается с принятым деревом;
control inventory обновляет только текущие status/roadmap hashes. Workflow,
checker, negative harness, правила main, legacy retirement и closed flags
не меняются. Это исправление stale inventory, а не ослабление проверок.
В main попадание возможно только через PR и required `rust`/`redis-smoke`.
До PASS нельзя заявлять main синхронизированным или обходить branch protection.

Не добавляются: reports/tmp, ELF/target, ZIP/handoff bundles, runtime DB/journals,
credentials/tokens, private bindings и ключи. В main уже есть три исторических
`reports/stage8b/*readonly-probe.txt`; они остаются побайтово прежними, а не
маскируются как новая evidence. Удаление принятой истории не входит в sync.
Операционные wrappers и final input manifests остаются в exact sealed execution
bundle по указанным SHA, а не неполной копией его каталогов в Git.

## Достаточный следующий объём

Получить bounded результаты, затем нужные длинные сессии/EOD и сравнение с
ALOR на одинаковом фактическом интервале. Historical freeze — reference, не
доказательство текущей live parity. Дополнительный обязательный счётчик сессий
не вводится. Новый paper framework не является текущим критическим путём.
FINAM order POST/DELETE, broker order dispatch, Redis DB0, runtime-live и
реальные заявки остаются закрыты. Auth session POST не является order route.
Git/CI synchronization не меняет установленный ELF, VPS, таймеры или authority.
