# WS-primary: короткая выдача BO-only и аудит коррекций

Статус: SOURCE REVIEW CANDIDATE, 05.11.2026. Установленный ELF, inputs, таймеры,
P0/O3, Redis/DB0 и реальные ордера не изменены. Это не operational acceptance.
Предшественник: accepted Git tree `6467abdbffb887a403e62ee3065961fc69132b96`
(именно tree, не commit). Исходный snapshot сохранён отдельно, без редактирования.

## Решение и явный opt-in

Для текущей свечной модели `imoexf-baseline07-bo-only-no-riskgate` вводится:

```json
{"primary":{"delivery":"ObservedBo1sVolumeAuditV1"}}
```

Это фрагмент изменения полного существующего config, НЕ готовый config/inputs.
Параметры модели и close-fill feedback не меняются. Новый delivery входит в
policy/config/semantic fingerprints. Старый ELF отвергает неизвестное поле.
В новом исходнике отсутствие поля означает `StrictBoundaryV1`: прежние правила,
сериализация и fingerprints старого config сохраняются. Неизвестные значения,
null и bool не принимаются. Разрешения счёта/readonly и candle-only не ослабляются.

Принятый `base.lmax_ms=10000` остаётся внешним защитным потолком, но opt-in имеет
более узкий effective live budget **2000 ms, включая clock uncertainty**. При
uncertainty=100 ms самая поздняя допустимая фактическая граница — C+1900 ms.
Поле exception_wait_ms=3000 относится к старому режиму; новый delivery явно
фиксирует effective proof wait=1000 ms и сужает deadline, не расширяя старые caps.

## Доставка модели

1. Полные 10 M1 той же физической generation, подписка до начала M10, реальная
   следующая boundary M1 и прежняя секундная grace дают `WS_LIVE`. REST не нужен.
   При boundary в C+100 ms готовность наступает в C+1100 ms. Цель — 1–2 секунды,
   но это НЕ обещание планировщика/провайдера доставить каждую свечу вовремя.
2. Если boundary отсутствует, вход неплотный, подписка началась внутри M10 или
   это SessionEnd, один source-admitted REST proof можно запросить уже в C+1000.
   Deadline — минимум существующих ограничений и C+2000-uncertainty. Нет запросов
   на +6/+9/+20 и серии повторов. Готовая свеча сохраняет `REST_EXCEPTION`,
   реальные request/receive времена, raw proof и hash. Она не объявляется WS.
3. Если бюджет упущен, новый LIVE callback не разрешён. Остаётся существующее
   source-bound восстановление, с историческим provenance и без выдачи replay за
   своевременный сигнал. CallbackFence, результат, publication/commit и reader
   проверяют более узкий budget. Уже мутировавший/ошибочный callback не повторяется.
4. Sparse M10 по-прежнему требует REST-подтверждения фактического состава.
   Пустой M10 не разрешается; отсутствующие M1 не заполняются нулями. Таймер не
   превращает десять полученных строк в доказательство полной живой подписки.

## Исправления объёма

Только при совпадающих instrument, timeframe, open/close timestamps и всех OHLC
допустимый числовой volume считается недецизионной коррекцией текущей BO-only.
Допускается как увеличение, так и уменьшение; max(volume) не назначается истиной.
Отрицательные/дробные объёмы, неверные цены, identity и timestamps по-прежнему
отвергаются исходным numeric/wire/source admission.

- До выдачи вход выбирается обычной source policy: актуальный provisional WS
  либо целый admitted REST bucket для исключения. Из полей разных источников
  не собирается искусственная свеча.
- После фиксации входа audit не меняет canonical input, digest, journal, модель,
  feedback или позицию и не вызывает callback повторно.
- Поздняя WS volume-only строка для committed M1 не останавливает контур, но и не
  освежает `last_live_data`: старый бар не доказывает свежесть текущего потока.
- В REST audit расхождение маркируется **VolumeRevision**, а не Match.
  Результат `VOLUME_REVISION` также отличён от MATCH в proof/comparison telemetry.
- Bootstrap-reference и recovery overlap используют ту же узкую совместимость.
  Уже обработанный overlap не воспроизводится заново. Новые historical inputs
  по-прежнему побайтно/численно связаны с выбранным admitted REST source.
- OHLC/inventory-конфликт остаётся основанием HOLD. Изменение high/low при обычном
  формировании ещё открытой M1 допускается прежними monotonic правилами; это не
  разрешение переписывать OHLC уже использованной закрытой свечи.

`primary_source_change` сохраняет контекст, generation, время записи, пару
нормализованных OHLCV, minute, observed source/frame hash и запрет переписывания
model input. Raw admitted REST сохраняется существующим journal. Очередь записей
ограничена max_cache_m1; ошибка записи/переполнение не превращаются в успех.
Время записи не выдаётся за время исходного получения WS. Исходные clocks/raw
сохраняются в source/proof receipts; нового механизма хранения тиков нет.

## Проверка независимости BO от volume

Новый metamorphic test прогоняет неизменённую модель и настоящий candle feedback
на 7307 frozen M10 в трёх вариантах: исходный объём, ноль и меняющийся большой
объём. Сравниваются все requests, полная observation до/после feedback; получено
86 feedback событий в каждом варианте, без различий. Дополнительно в текущем BO
kernel volume проходит numeric checks/DTO, но не участвует в решающих формулах.
Это обоснование ТОЛЬКО для закреплённого BO-only профиля, не для иной стратегии,
quotes-based fills, volume filters/VWAP, live execution или будущих изменений.

## Обзор соседних причин остановок

| Событие | Реакция | Решение этой итерации |
|---|---|---|
| Late volume-only WS / REST | Аудит, без повторного callback | Исправлено только для opt-in |
| Reference/recovery overlap отличается только volume | Совместимость, исходный model prefix неизменен | Исправлено и проверяется reader |
| Quiet boundary / StartupPartial / sparse | Один ранний REST proof; без выдуманных M1 | Новый deadline/выдача, не обход admission |
| Audit Timeout/Unavailable | Аудит недоступен, WS продолжает | Существующее поведение сохранено |
| EOF/Close/Read/Send/Connect, временная auth Unavailable, liveness timeout | Закрыть LIVE fence, reconnect/resubscribe + source-bound resync | Существующее bounded recovery сохранено |
| Бюджет выдачи пропущен | Не публиковать поздний LIVE; историческое восстановление | Сужен до 2s для opt-in |
| Повтор той же уже принятой строки | Dedup без callback/volume суммирования | Сохранено |
| Противоречащий OHLC / пропавшая подтверждённая M1 / identity | HOLD с диагностикой | Не ослаблено |
| Неверный account/readonly/permissions, auth Integrity/Clock/Denied | Терминальный отказ | Не ослаблено |
| Неверные ticks/OHLCV, future/unaligned timestamps, generation/owner | Отказ до модельной мутации | Не ослаблено |
| Очередь переполнена / stale ingress | LIVE fence + существующее восстановление | Лимиты не расширены |
| Disk/journal failure, callback/feedback failure/panic, нарушенный порядок commit | Терминальный отказ, без повторной мутации модели | Не ослаблено |
| STOP/deadline/EOD, исчерпаны finite recovery budgets | Завершение; без автоматического нового прогона | Не ослаблено |

Проверены существующие recovery caps: максимум 3 эпизода на run, до 3 попыток
в эпизоде, backoff 1/2/4 s, общий episode budget 45 s. Heartbeat: ping 30 s,
matching pong 15 s, transport idle 70 s. Отсутствие BARS 60 s даёт предупреждение,
а не немедленную смерть; hard data silence — 1200 s. Pong не доказывает свежесть
рынка. В этой итерации эти числа и механизм reconnect/resync не менялись.

Оставлены спорные места: unexpected snapshot после fresh DATA, исчерпание
recovery и непредвиденное отсутствие completion от HTTP executor. Их нельзя
без проверки провайдерского контракта превращать в бесконечный retry или чистый
успех. Для текущего малонагруженного shadow достаточно существующих ограничений.
Полный operational soak новой политики ещё не выполнен.

## Утренний инцидент и границы доказательства

05.11 в 07:10 после StartupPartial proof был CanonicalConflict до model callback.
Поздний REST показал изменение volume 07:09: 1543 -> 1548, OHLC не изменились.
Но конкретная отвергнутая WS row не сохранилась: это кандидат причины, НЕ её
доказательство. Тест 1548/1543 — синтетическая регрессия этого класса расхождений.
Read-only smoke 08:25–08:26 совпал во всех четырёх snapshots и не доказал finality.

Отдельный operational пробел — поздний systemctl show может потерять exit status
уже выгруженного unit. Durable exit witness до unit GC нужно закрепить в следующем
release/execution комплекте; в этой SOURCE-дельте installer/units не изменялись.

После SOURCE ACCEPT: Linux release + квалификация, затем один согласованный
bounded operational run с новым config fingerprint и устойчивым exit witness.
Новая установка/активация, FINAM-трафик, ALOR parity, live-micro и реальные сделки
этим документом не разрешаются и не считаются доказанными.
