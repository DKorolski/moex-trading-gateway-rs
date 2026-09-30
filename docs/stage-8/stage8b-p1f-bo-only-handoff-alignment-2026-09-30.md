# ALOR handoff → FINAM: no-riskgate target and short warmup

Дата: 2026-09-30. Статус: документальная сверка и план следующего source slice.
Сверка ниже фиксирует исходное состояние до коррекции. Теперь подготовлен
[source review candidate](stage8b-p1f-no-riskgate-source-correction.md);
независимый source acceptance нового профиля ещё не получен, VPS не менялся.

Решение закреплено в [ADR](../adr/adr-stage8b-bo-only-no-riskgate-short-warmup.md).
Оно уточняет будущую разработку после принятого baseline, а не переписывает
прошлые reviews или immutable handoff.

## 1. Источники и границы достоверности

Владелец проекта 30 сентября сообщил: **сейчас на ALOR нет систем, использующих
riskgate**. Это актуальное требование для разработки. В рамках этой сверки
удалённые ALOR/VPS не опрашивались; текущие live-конфиги независимо не снимались.

Повторно проверен `imoexf_live_shadow_finam_freeze_2026_09_27.zip`:

- SHA-256: `7ad5433b88fb7ba835b3a57603a9d773a6c4a2f5dd51fe9e37c8cbed5b5f534e`;
- 37 members, ZIP CRC PASS;
- guide и пять model/gateway TOML из таблицы ниже совпадают с распакованными
  локальными файлами byte-for-byte.

Корень handoff относительно рабочего каталога проектов:
`analiz_alpha_si/imoexf_live_shadow_finam_freeze_2026_09_27/`.
Связанный [исторический intake](stage8b-p1f-alor-finam-freeze-intake-2026-09-27.json)
и [source correction от 27 сентября](stage8b-p1f-alor-finam-source-correction-2026-09-27.md)
сохранены без изменения. Их retained High180 accounting описывает прежнее решение,
а не новое требование владельца.

| Файл внутри freeze | SHA-256 |
|---|---|
| `FINAM_PARITY_GUIDE_RU.md` | `e424f4451071ce9c1321e76276851b98d462e35ee03011f0bd84bf5b9a201c28` |
| `reference_deployment_DO_NOT_RUN/trading-hybrid-alor-gateway-1.toml` | `602b7283708542243db830f3960d7643a64edcaebdc8ca7a0af812873042884e` |
| `reference_deployment_DO_NOT_RUN/trading-hybrid-strategy-runtime-1.toml` | `c38cd89cb7d76f8833302e8e572e5cc47177bec73b06dcdd710132e5d531737d` |
| `reference_deployment_DO_NOT_RUN/trading-ri-author41-42-7502miw-strategy-runtime-1.toml` | `380f3e31c7b187dee12ea2a783323a11386ee08346e635811385f9e4942dbaef` |
| `reference_deployment_DO_NOT_RUN/trading-alor-usdrubf-strategy-runtime-1.toml` | `4c98e1e3270c8c4e6c3c2089f1208351baec641a3b7916f1fa541170a21596f2` |
| `candidate/runtime.hybrid_imoexf.shadow_weekend_state.bo_only_weekend09_close_v1.7502MIW.toml` | `383715980f596a76eead0ef687b15886b0ddc5c03ec97326590a11979669e73d` |

Эти deployment TOML — только reference, не готовая безопасная конфигурация FINAM.
Их credentials paths, live flags, account/namespace и размеры позиции не копируются.

## 2. Модели не смешиваем

| Модель из freeze | Подтверждено файлами | Как учитываем в FINAM |
|---|---|---|
| IMOEXF baseline07, `hybrid_intraday` | BO-only, MR entries false; модель 07:00–23:49:59 MSK, выходные исключены; исторически `normal_append` / High180 / lb120 seed | Первая цель parity. Сохраняем BO-семантику, убираем обязательное shadow-riskgate accounting согласно актуальному решению владельца |
| IMOEXF candidate09, `hybrid_intraday` | BO-only; `risk_gate_mode=disabled`, `mr_gate_policy=disabled`; модель 09:00–23:49:59; weekend `state_only`; отдельный current-close paper fill | Отдельная исследовательская/paper модель. Её anchors, weekend policy и fill policy не подменяют baseline07 |
| RI, `ri_author41_42` | Author41/42 combo; 07:00–23:49:59; anchor completeness/transition rules; gateway history 6 sessions/days; riskgate-параметров в runtime TOML нет | Контекст будущего порта. Не навязывать High180/lb120; не переносить сюда четырёхсессионный лимит IMOEXF. Отсутствие параметров само по себе не доказывает все code defaults |
| USDRUBF, `alor_usdrubf_hybrid` | MR + BO параметры; 07:00–23:49:59; BO wait 2h, K=0.45; gateway history 4 sessions/days; riskgate-параметров в runtime TOML нет | Сохранять собственную MR+BO модель при будущем порте. «Нет riskgate» не означает «выключить MR»; полный port/parity в этот slice не входит |

У IMOEXF baseline07 и candidate09 одинаковые BO K=0.53, stop1=0.51,
stop2=0.35, big-move threshold=0.025, min-range=1.01 absolute и wait=3h.
Различается начало модельной сессии: ранняя модельная метка BO допуска — 10:00
для baseline07 и 12:00 для candidate09, доступность закрытого бара на 10 минут
позже. Это не обещание входа на первом допустимом баре.

Guide §5 прямо отделяет High180 shadow-ledger от BO signal parity. Поэтому его
наличие в историческом baseline TOML не делает riskgate обязательной частью
новой FINAM BO-only модели. Профиль без riskgate не считается точным воспроизведением
исторического High180 accounting; сравниваются нужные BO data/state/signal поля.

## 3. Почему установленный FINAM всё ещё требует апрель

Проверенный source/package HEAD:
`3923c5c94f27a1dc980297f289c10ecca652f99b`.
Установленный полный O2 artifact собран из
`589b80144adaa4c615aaa94781035d5a6af64c71`.

| Текущий источник | Фактическое требование |
|---|---|
| [Runtime profile](stage8b-p1e-runtime-profile-v1.json) | `live_mr_entries_enabled=false`, но semantic profile High180/lb120, `risk_gate_mode=normal_append` |
| [First-boot source](../../crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs) | `MIN_HISTORY_SESSIONS=121`, `MIN_RISKGATE_SESSIONS=120`, обязательные riskgate observations |
| [Materializer CLI](../../crates/finam-gateway/src/bin/stage8b-p1f-o2-materializer.rs) | Диапазон запроса минимум 180, максимум 400 дней |
| [Materializer](../../crates/finam-gateway/src/stage8b_p1f_o2_materializer.rs) | Вызов `rebuild_stage8b_p1e_riskgate_observations_v1`, проверка 121 session и exact M1 buckets |
| [Policy](stage8b-p1f-o2-materialization-policy.json) / [source template](stage8b-p1f-o2-source-template.json) | Query 2025-09-27…2026-10-31; 121 явная history session 2026-04-10…2026-09-25 |

Это ещё не исправлено. Одной правки даты запроса или TOML недостаточно: admission,
constructor, persistence/profile binding и materializer должны согласованно
поддерживать новый профиль. Нельзя обойти проверки старого контракта.

В сохранённой попытке 30 сентября materializer отклонил отсутствующий M1
`2026-04-10T13:27:00Z` (16:27 MSK), exit 70; bootstrap не запускался.
Cleanup завершён с `FAILED / generation 1 / sequence 4`, прежняя terminal history
сохранена, P0 не изменён, DB15 пустая. Это retained evidence, не новый live snapshot
и не доказательство причины ещё более раннего `BarsTruth`.

Evidence ZIP: `finam-o2-3923c5c-bounded-successor-evidence-20260930.zip`, SHA-256
`527d20dbc23079d9151f6868c81cab135b2db5c53fac2dce24fce64a83cac2d7`.
Пакет подготовлен, независимый acceptance этой попытки не заявляется. O2 — HOLD.

## 4. Следующий узкий source slice

1. Версионировать baseline07 BO-only no-riskgate profile и его source requirements.
   MR entry по-прежнему запрещён до ownership/pending. Disabled accounting должен
   быть явным, без фиктивных 120 строк, seed/ledger import или «успешного нулевого
   riskgate». Shared legacy riskgate code и старые accepted fixtures не удалять.
2. Разделить history warmup и riskgate rebuild. ALOR IMOEXF gateway фиксирует
   `history_sessions=4`, `history_days_back=4`, `cold_start_history_days_back=4`.
   Это ориентир для bounded short warmup, а не доказательство достаточности любых
   четырёх календарных дней. Выбирать недавние допустимые сессии по календарю;
   доказать prev-close/range/return, prev-session date, current-session context
   и BO readiness. Точное окно и конечный предел query закрепить в source-тестах
   и replacement policy; не раздвигать окно до 121 сессии автоматически.
3. Для no-riskgate пути убрать обязательные High180 reconstruction / 120-session
   observations и 180-day lower bound. Сохранить bounded query, exact required
   coverage, provenance, freshness и календарь. Missing M1 внутри требуемого
   недавнего окна остаётся отказом; отсутствие апрельских данных вне него не
   должно вызывать запрос/отказ. M10 из freeze не превращать в придуманные M1.
4. Зафиксировать совместимость restore: новые profile/source fingerprints,
   отказ при смешении старого riskgate-enabled package с новым профилем;
   сохранение pending/request identity, broker truth, ACK/truth seals и XACK-last.
   Не считать старый operational root новым и не стирать terminal history.
5. Подготовить один завершённый source handoff с focused regression evidence.
   **Reviewer подключается после этого source slice**, до rebuild/установки нового
   исполняемого artifact. Предмет review — новая граница требований, а не повторный
   аудит всей принятой истории без признака регрессии.

Минимальные проверки для source acceptance:

- fresh no-riskgate BO-only bootstrap на короткой полной истории без seed/ledger;
- явное отсутствие вызова High180 rebuild и обязательного lb120 admission;
- short-window anchors/BO decisions совпадают с oracle на одинаковых входах;
  weekend/holiday/start-of-session и intraday cutoffs не используют будущие OHLC;
- warmup не отправляет исторические intents, MR не захватывает owner/pending;
- missing/duplicate/out-of-order/non-finite данные и stale truth в требуемом
  окне не обходят существующие проверки;
- long offline baseline07 regression сохраняется; новый no-riskgate профиль
  проходит её по BO-сигналам, отдельно от paper execution/fill сравнения;
- restart exact pending/request IDs и semantic commit/XACK-last сохраняются;
  legacy riskgate-enabled evidence не получает послаблений;
- свежее связывание profile/source/artifact identities и закрытые live surfaces.

После source acceptance: новый exact artifact и проверка установки с сохранением
terminal history → отдельно разрешённый bounded O2 → O3/O4 paper sessions →
сравнение с актуальным ALOR той же модели → отдельный gate live micro.
Для сравнения реальных сессий снять актуальные ALOR model parameters/identity,
не выдавая freeze от 27 сентября за сегодняшний live snapshot. Другие модели
портировать отдельными slices, не задерживая первый IMOEXF paper этим переносом.

Исходная сверка была docs-only; последующая source-коррекция описана отдельным
документом выше. VPS, старые runtime profile/source template/policy, manifests и
архивы остаются неизменными. Нового разрешения
на O2, Redis activation, FINAM order POST/DELETE, broker dispatch, runtime-live
или real orders этот документ не содержит. Дополнительный recovery framework
для этой коррекции не нужен.
