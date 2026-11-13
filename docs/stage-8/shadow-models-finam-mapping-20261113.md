# FINAM mapping трёх shadow-моделей — 13.11.2026

Статус: observed read-only collection + offline validation PASS, PENDING REVIEW.
Это не release/execution acceptance и не capability допуска рыночной M10.
Accepted local M10/SQLite baseline: 3755928dd3e3cdc7a63c303561769f5faf2a7a40.
Оба прежних M10P findings закрыты отдельным SOURCE ACCEPT; здесь их не открываем.

## Подтверждённые записи

| Model | FINAM provider symbol | Broker asset ID | Tick | Reference lot_size | trade_lot_size | Expiry MSK |
|---|---|---|---|---|---|---|
| IMOEXF | IMOEXF@RTSX | 4631091 | 0.5 | 10.0 | 1 | не сообщена |
| USDRUBF | USDRUBF@RTSX | 3447194 | 0.01 | 1000.0 | 1 | не сообщена |
| RIZ6 | RIZ6@RTSX | 5436517 | 10 | 1.0 | 1 | 2026-12-17 |

Имена/ID взяты из настоящего FINAM AllAssets, а не построены из предположения.
Для RIZ6 ticker=RIZ6, display name=RTS-12.26, MIC=RTSX, board=FUT, type=FUTURES.
Передача RTS-12.26@RTSX вместо реально возвращённого RIZ6@RTSX не нужна.
Metadata GetAsset подтверждает id/ticker/mic/type/name той же каталоговой записи.

Tick рассчитывается как min_step / 10^decimals, без float arithmetic:
IMOEXF 5 / 10^1 = 0.5; USDRUBF 1 / 10^2 = 0.01; RIZ6 10 / 10^0 = 10.
Нельзя использовать необработанное min_step=5 как tick IMOEXF.

Reference lot_size и reported contract_size сохраняются отдельно от
trade_lot_size, полученного для выбранного счёта. Все trade_lot_size равны 1:
один модельный виртуальный контракт не превращается в 10 или 1000 ордерных
единиц из-за reference lot_size. Экономический point value RI, комиссии,
конвертация валют и округление будущих реальных исполнений этим не выводятся.
Принятые fees/feedback/формулы моделей не менялись.

Structured expiry RIZ6: {year:2026,month:12,day:17}.
future_details.expiration_date: 2026-12-16T21:00:00Z, то есть 17 декабря по МСК.
Это не противоречие дат. Дата совпадает с pinned RI profile SHA-256
30e74209c058e288ab171afb7d18cba4d9bb93a0e4927cf9a6062325e96fe989.
Из такого date-valued timestamp не выводится время окончания торгов или STOP.
Автоматический rollover не реализуется и не разрешается. Для IMOEXF/USDRUBF
expiry в полученных metadata отсутствует: это честно recorded как not_reported,
а не как доказательство бессрочности или самостоятельно выбранная дата.

## Как получено

Непосредственный локальный сбор, без загрузки helper на VPS:
POST только /v1/sessions и /v1/sessions/details (существующая auth-схема),
GET /v1/assets/all?only_active=true, затем GetAsset/GetAssetParams трёх
обнаруженных symbol. Origin фиксирован https://api.finam.ru; прокси и redirects
отключены. Нет POST/DELETE order endpoints, аккаунтных позиций/денег/сделок,
WS, bars, Redis, strategy callbacks, установки или запуска новых контуров.

Секрет прочитан из существующего .env только в память, не передан через argv
или shell export. Проверены readonly=true, selected-account membership,
RTSX zero-delay permission и настоящий expires_at с запасом 120s.
Истёкший после завершения сбора access token не переиспользуется.
API is_tradable=true — описательный параметр выбранного счёта, не разрешение
отправки ордеров из shadow или из read-only credential.

Первая auth-попытка завершилась timeout. Следующая авторизация прошла, но
/v1/assets вернул 500. Документированный AllAssets использован как альтернатива.
При первоначальной пагинации ответ страницы 19 не прошёл Content-Length:
эта невалидная response body не использована. Следующий явный сбор продолжил
строго проверенный prefix из 18 raw страниц, сверив hashes/cursor chain.
Получены ещё страницы 19–21 и точные metadata. Критерии полноты ответа не
ослаблялись; сервисы и конфигурация действующего V6 не менялись.
SSH использовался лишь для сетевой read-only проверки uname, не для сбора.

Сканирование остановлено после нахождения всех трёх target records:
21 валидная страница, 63 000 catalog rows. Это prefix активного каталога,
catalog_complete=false, НЕ полный аудит мирового instrument registry и НЕ
атомарный snapshot всех записей на один момент. Три выбранные записи ровно
один раз найдены в scanned prefix и отдельно подтверждены metadata.
Часы всех запросов/ответов сохранены, не перенесены к будущему запуску.
Последние metadata получены 13.11.2026 в 22:33:39.262 МСК.

## Evidence и воспроизведение

Snapshot: shadow-models-finam-mapping-20261113.json.
Его evidence_files относительны к evidence/collection в review ZIP.
Все raw catalog pages сохранены с SHA-256, HTTP status/bytes/Content-Length
и original request/receive clocks, включая непрерывную cursor chain.
Для account-dependent asset/params сохранены public projection + original
response SHA, а не якобы исходная raw body. account_id, secret, access token
и auth bodies не сохранены в Git/ZIP/logs. Нельзя называть projection полной
raw response или делать из её JSON admitted producer capability.

Offline verifier проверяет согласованность представленных данных, hashes,
identity, единицы и даты; он не выдаёт криптографическую подпись FINAM и сам
по себе не доказывает сетевое происхождение произвольного подложенного JSON.
Сетевое observation evidence связано с выполненным collector, его exact source
и сохранёнными request receipts. Закрытые поверхности явно остаются false.

```sh
python3 -B -m unittest discover -s tests -p test_finam_shadow_catalog.py -v
python3 -B scripts/finam_shadow_catalog_verify.py \
  --collection /absolute/package/evidence/collection \
  --prefix-record /absolute/package/evidence/prefix-collection.json \
  --output /absolute/new-mapping-snapshot.json
```

12 offline tests включают tick scaling, lot distinction, expiry timezone,
неправильный contract/ID/MIC/board, readonly/account/expiry отказы, отсутствие
shell execution, private-field guard, изменение raw hash/projection/cursor
и запрет лишних POST/activation. Все unit traces синтетические; реальная
collection отдельно содержит фактические FINAM responses.
Crates, Cargo, CI и runtime binaries не менялись: повторная большая V6 CI
не является gate этого mapping/evidence среза.

Официальные первичные контракты:

- [FINAM: instrument metadata и лоты](https://api.finam.ru/getting-started/bystryy-start/nayti-instrument-i-proverit-torgovye-parametry/).
- [FINAM OpenAPI: AllAssets/GetAsset/GetAssetParams и scaled min_step](https://github.com/FinamWeb/finam-trade-api/blob/main/docs/swagger/api.swagger.json).

Далее: после review использовать mapping при trusted producer bridge, сохраняя
original source clocks/construction и разделение replay/observed. Затем
continuous-session/rollover с сохранением pending/position, отдельные
Linux release/execution gates. Этот срез не начисляет clean sessions и не
разрешает operational активацию новых моделей.
