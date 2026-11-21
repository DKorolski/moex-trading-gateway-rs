# USD/RI managed V8 R1 — MGR-1 source-last evidence

19 ноября 2026. Scope: узкое source исправление matching reader.

Reviewed R0: `dffd2bd15e4b66fe50e48f79bbdcd9b1ba028248`, SOURCE CHANGES REQUIRED.
Accepted foundation остаётся `342a3cbf7a2720e0cbd09bc014192fef646ffedb`.
R0 не объявляется принятым задним числом.

## Замечание и воспроизведение

MGR-1/P2 подтверждён настоящим неизменённым R0 Rust CLI: positive и все три
resealed reviewer mutants (omitted / early / foreign SHA advance) возвращали
exit0 / MANAGED_PREFIX_VERIFIED. SQL/manifest/control не менялись при чтении.
Сохранены stdout/stderr, hashes и identity родительского бинарника.
Reviewer Python/patch не выполнялись; использованы данные его evidence.

## Исправление

Reader теперь отдельно учитывает для каждого инструмента:

- SQL checked-readback frontier;
- ожидающее source продвижение с точной Candle и readback clock;
- фактически подтверждённый advance frontier и закрытие линии.

Каждый `primary_models_source_advanced` обязан идти после matching readback
и до следующего input этой же линии. Ровно одно advance сверяет instrument
через его pending Candle, label, exact source receipt SHA, source_committed,
next_label (включая обязательное явное null после последнего M10).
Раннее, повторное, пропущенное, чужое, перескочившее и post-final продвижение
не заменяются последним суммарным счётчиком.

Для actual `source_advanced_at_ms` проверяются readback <= actual <= recorded,
монотонность своей линии, bounded stop и прежние бюджеты: LIVE + U100 <= 2s;
historical <= exact replay deadline и <= ready + 10s; WS historical также
строго раньше episode deadline. Используется canonical origin, а не наличие
необязательного recovery-поля. Время записи event не подменяет actual clock.

Другая линия может записать свой SQL/readback до delayed advance log первой.
Глобального запрета двух per-source pending advances не добавлено.

Final source frontier и RunReport теперь сопоставляются с per-label advances,
не только с количеством SQL readbacks. При настоящем HOLD до native advance
допустим один незавершённый intent/readback и SQL-ahead; native frontier остаётся
прежним. Нельзя оправдать удалённое advance одним выставленным held=true, если
final native frontier уже заявляет продвижение. Без post-join completion —
INCOMPLETE. JSON по-прежнему не создаёт native capability.

В inspection `source_counts` означает actual verified advances;
`checked_source_counts` отдельно показывает проверенный SQL prefix.

## Доказательства R1

Новые тесты генерируют CURRENT-identity native Duplex fixtures, затем reseal
только control. Отказы обязаны происходить по source-advance predicates,
не по устаревшему implementation hash, metadata, SQL или SHA-цепочке.

Сохранены 16 source-last mutants, позитивное независимое interleaving,
реальный native STOP-after-SQL с нулём advances, его synthetic checked-readback
вариант и INCOMPLETE без completion. Отдельный pure DTO test проверяет точные
LIVE/replay/episode/stop границы; это не имитация native M10 capability.
Существующие два managed Duplex, reconnect и 28 CLI negatives сохраняются.
Точные итоговые gates/counts находятся в immutable review package.

Production delta — только managed reader. Producer, Store/SQL/native advance,
transport/auth/STOP/recovery implementation, numerical kernels, profiles,
fees, broker, Cargo и workflows побайтно не меняются. Test helper только
сохраняет свои fixtures и использует уже существующий cfg(test) fault hook.

Это не executable/input glue, Linux/native release или operational acceptance.
VPS/FINAM/keys/DB0/timers/installed IMOEXF/main/origin не затрагиваются.
После source acceptance можно продолжать согласованный следующий source-срез.
