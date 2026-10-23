# Reviewer: согласование signal-shadow scope и запрос узкого ТЗ

2026-10-23. Тип пакета: planning / scope review, НЕ source acceptance.

Просим согласовать смену ближайшего приоритета: остановить расширение
незавершённого paper/O4 и подготовить изолированный FINAM signal-shadow
для сравнения с ALOR на существующем VPS. Paper сохраняется для возможного
возврата. Цель — необходимый и достаточный путь к parity, затем отдельно
авторизованному micro-live, без нового общего recovery framework.

## Читать в таком порядке

1. `docs/stage-8/stage8b-signal-shadow-priority-decision.md` — решение владельца,
   предложенный scope и границы.
2. `docs/stage-8/stage8b-p1f-o3-bounded-closure.md` и оригинальный review
   в `docs/stage-8/reviews/` — принятая исходная граница.
3. `docs/adr/adr-stage8b-bo-only-no-riskgate-short-warmup.md` — модель и прогрев;
   `stage8b-p1f-bo-only-handoff-alignment-2026-09-30.md` и ALOR guide —
   различия исторического freeze и выбранного no-riskgate профиля.
4. `docs/stage-8/stage8b-operational-market-data-parity-checklist.md` —
   WS, реальные данные, своевременность, EOD.
5. `docs/current-status.md`, `docs/roadmap.md`, resumption plan и O4 preparation —
   справочная история. Старые «next step» не отменяют решение от 23 октября.
   Длинный O4 progress log не требует построчного повторного review.

## Что просим вернуть

- Короткий verdict по смене scope, без повторной приёмки всей истории.
- Узкое ТЗ на source slice: входы/выходы, существующий entry point либо
  необходимый adapter, затронутые модули, минимальная acceptance matrix.
  Если для точного выбора entry point нужен код — запросить отдельный source
  snapshot; этот docs-only пакет не подтверждает пригодность реализации.
- Явный контракт shadow state и model feedback: что делать с intents, как
  виртуальная позиция влияет на последующие сигналы, какие fill assumptions
  допустимы. Не смешивать это с реальной ALOR-позицией или retained paper +1.
- Способ инициализации общей предыстории/состояния, дедупликации и restart;
  разделение warmup/replay и наблюдений live-времени.
- Минимальные критерии наблюдения (предложены две полные сессии с EOD/следующим
  стартом), разбор расхождений, условия остановки и достаточность evidence.
- Отдельные gates: source acceptance → разрешённая установка/наблюдение →
  parity report → минимальная execution safety перед micro-live.

Просим не делать весь отложенный O4 backlog обязательным prerequisite
signal-shadow. Если конкретная часть необходима для правильной семантики,
указать зависимость и минимальный тест. Неизменяемые принятые материалы
переоткрывать только при конкретном regression signal.

## Что этот пакет не утверждает

O2 и bounded synthetic O3 приняты; operational parity не доказана.
O4 NOT_RUN / source unaccepted, локальная работа сохранена. Текущее состояние
VPS повторно не проверялось. Никаких установок, запусков, reset/flatten,
FINAM POST/DELETE, broker dispatch или реальных заявок этим пакетом не разрешено.
Существующие P0/O2/O3, DB15, история и paper +1 не изменены.

Пакет содержит снимок документов из dirty worktree, а НЕ `git archive HEAD`.
HEAD/branch и полный список dirty paths записаны в `package-provenance.json`
и `working-tree-status.txt`. Исходники, бинарники, ключи, .env, сырые рыночные
выгрузки и прежние большие ZIP не включены. Локальные результаты тестов в
исторических документах не повторялись при этой упаковке.
SHA-256 каждого вложения записан в manifest; ZIP SHA-256 и структурная safety
передаются отдельно. Структурная safety не является source/runtime acceptance.
