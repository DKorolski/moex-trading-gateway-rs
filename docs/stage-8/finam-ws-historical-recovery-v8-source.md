# FINAM shadow: WS historical recovery V8

Дата: 17.11.2026. Статус: source candidate для независимого review.

Baseline: commit 61375b0c3fbd5feb7f20e69f840c844cb1daff83,
tree fcda4779442aafa7097735096f5e856b018f5acd (принятый V7).
Основание: FINAM_V7_WS_GATE_REVIEW_AND_TASK_20261117_RU.txt.
Завершённый run ws-primary-cgroup-v7-20261117-0700-r0 остаётся HOLD;
повторный запуск старой попытки не разрешён.

## Изменение

Только explicit opt-in: ObservedBoWsHistoricalRecoveryV8. Default и V1–V7,
их сериализованные identities и параметры не переименованы.

Когда LIVE deadline уже прошёл, полный current-generation WS-кандидат может
завершить bounded data-only recovery. Новый source — WS_RECOVERY_BACKFILL:
source_transport=WS, processing_mode=BACKFILL_REPLAY, timeliness=LATE,
broker_execution=false. REST snapshot/proof для него отсутствуют, а не
подставляются из параллельного GET.

Не меняются grace=1000 ms от max(close,boundary), LIVE=2000 ms,
configured uncertainty=100 ms, request=5000 ms, episode=45000 ms,
max Recovery GET=4, backoff=1/2/4 s, run/EOD/STOP, другие лимиты.
На исходной форме B=C+1364 зрелость остаётся C+2364, а не C+1364.

## Разрешение и единственный writer

PrimaryProcessor создаёт reason-bound DataGap только на пересечении live
publication deadline, если до gap уже сохранены все десять текущих M1:
непустой физический owner, subscription до label, одна generation, нет
quarantine, in-flight admission или terminal SessionEnd. Повторный data-only
latch сохраняет эту узкую capability; break_continuity, ingress loss и новая
generation уничтожают её. Unknown owner и arbitrary fresh tick её не создают.

Единственный writer дополнительно проверяет continuity revision, отсутствие
физической ошибки, актуальный episode и транспортную готовность. Он оценивает
поздний WS на Frame и Tick, в том числе во время REST/backoff. Readiness timer
конкурирует только при выборе события; future уже отправленной команды writer
не отменяется. Original boundary, first/latest frame hashes и времена версий
сохраняются отдельно.

Один unpublished label — один source winner. REST admission и WS admission
сериализованы. WS selection заменяет pending REST capability; поздний REST
ответ сохраняется/дренируется как stale diagnostic без нового callback.
Если целый допустимый REST выбран первым, WS не получает второе разрешение.
Producer frontier меняется только после model + feedback + durable commit.
При STOP/ошибке/неопределённой мутации продолжения нет.

После четвёртого GET новых GET нет. V8 healthy data-wait может дождаться WS
внутри того же episode; deadline не переносится и бюджет не обнуляется.
Если надёжного источника нет — bounded HOLD. Настоящий transport gap
по-прежнему требует принятого REST recovery.

## Модель и независимый reader

Продолжается существующая BO-модель и виртуальная книга. Общий исторический
close-fill путь используется и REST, и новым typed WS input. Исторические
entry/exit intents получают штатный shadow feedback; слово historical не
подавляет сделки и не разрешает broker dispatch. Pending/held mutation
не допускается. Committed prefix и строгие правила неизвестных ревизий не
переписываются.

Virtual close clock модели отделён от actual ready/model/result/publication
времён. Independent restart/EOD reader пересчитывает typed provenance digest,
M1/M10 Decimal math, исходный grace, live lateness, subscription/generation,
DataGap/episode/revision lineage, источник-победитель и durable commit.
Receipt от V8 отвергается при прежней policy.

В V8 EOD-результате delivery_evidence отдельно показывает полноту inventory,
число historical WS и всех historical model inputs, а также
all_inputs_published_live. Полнота модели не означает своевременность доставки,
network/cgroup qualification, ALOR parity или новое правило clean-session credit.

## Адресные доказательства

Тесты v8_* охватывают incident-shaped 10/10, B=C+1364, Proof 2/10,
Recovery 22/30; WS/REST/near-simultaneous winners; readiness без нового WS;
drain позднего REST; STOP; 4 GET без пятого; expiry исходного episode;
incomplete/mixed/quarantined/startup/unknown owner/in-flight/clock/ingress/
bad/history WS/generation negatives; реальные BO entry/exit и pending refusal.

Сжатый synthetic day проходит 101 M10, одна из которых — WS historical,
с terminal SessionEnd через прежний REST путь, завершением workers и независимым
EOD reader. Prefix reader проверяет restart admission; затем реальный
PreparedEntry.open реконструирует новый owner/model из префикса с late WS:
проверяются идентичность итогового state/decision, frontier и отсутствие
восстановленной transport readiness. Resealed negatives
меняют generation, boundary/grace, actual ready, revision, origin/timeliness,
broker_execution и дублируют callback при заново посчитанных outer hashes.

Последующий WS cache и положительное продолжение являются SYNTHETIC.
Оригинальный operational result не содержит полного raw WS timeline после
последнего snapshot: успешный альтернативный FINAM replay или полный день
из него не объявляется.

## Безопасная следующая граница

Сначала независимый source review. Затем отдельно exact-source Linux runtime
и reader release, qualification и новый execution-пакет/новый run ID.
В этой source-работе VPS, installed V7, таймеры, P0/O3/DB0, ключи,
FINAM API, main и origin не меняются. CI/governance acceptance принятой
истории данным документом не переоткрывается и не подменяется.
