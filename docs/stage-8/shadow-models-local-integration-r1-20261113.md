# Общий M10 / SQLite: узкий source R1

Baseline: a7b2c63650cb22fdb3f175bcc3a6f5168ab889ee.
Review от 13.11.2026: SOURCE CHANGES REQUESTED по M10P-1/P2 и M10P-2/P2.
Статус этой коррекции: кандидат на повторный SOURCE REVIEW, не приёмка.
Accepted numerical baseline 01769820 и working V6 не пересматриваются.

## M10P-1: outcome SQL key ↔ payload

Общий decoder получает SQL arm/source_sequence вместе с result/result_sha.
До возврата проверяются checksum и равенство обоих SQL-ключей полям Outcome.
Тот же decoder используется в causal outcome-group read и latest outcome read;
последний дополнительно связывает sequence с checkpoint frontier.
Остальные сверки input/revision/chain/state/marks сохранены.

Точный reviewer mutant sequence 313 → 314 при неизменённых JSON/hash и всех
других таблицах должен отклоняться уже при open/export с причиной
outcome SQL index/payload mismatch. Соседний mutant подменяет SQL arm на
существующий USDRUBF/weekday_only: foreign key остаётся допустимым, decoder
всё равно обязан отказать. Автоматического ремонта или переписывания нет.
Нормальный контроль: create → first apply → reopen → identical duplicate →
next valid input; duplicate не выдаёт marks заново.

## M10P-2: одна точная область для wire arithmetic

Для всех decimal wire-полей (tick, OHLC, volume) принимается только ASCII
грамматика `-?[0-9]+([.][0-9]{1,9})?`. Длина строки не более 48 байт.
Exponent, plus sign, whitespace, separators, NaN/Inf, одинокая точка и дробная
часть длиннее 9 знаков отвергаются. Leading zeros допустимы, -0 означает 0.
Знаки и OHLC/volume проверяются затем над точным значением как раньше.

Точное представление: i128 coefficient в единицах 10^-9, симметричная область
[-i128::MAX, i128::MAX]. Максимальная абсолютная величина:
170141183460469231731687303715.884105727.
Коэффициент строится checked_mul/checked_add без промежуточного decimal parser
и без rounding. Выход за область — ошибка, не saturating/truncating conversion.
Tick/remainder и сравнения OHLC используют те же целые единицы.
Volume суммируется checked_add; переполнение даёт явный отказ до model/commit.
Граничные большие volumes — арифметические тесты, не реалистичный market input.

rust_decimal больше не используется; зависимость удалена вместе с её
неиспользуемой транзитивной lockfile-частью. Новых зависимостей нет.
SQLite, транзакционный протокол и numerical kernels не менялись.

Переход в существующий f64 facade выполняется из канонической десятичной строки
уже проверенного ExactDecimal. Сырой wire-текст повторно как f64 не разбирается.
Обычная binary64-квантизация на границе принятого numerical API остаётся:
это НЕ обещание точного представления decimal 0.1 в IEEE754. Заявленная точность
касается parsing/tick/aggregation ДО этой явной конверсии, где нет скрытого
округления или расхождения двух parser paths.

## Regression contract

В suite добавлены пять тестов (65 вместо 60):

- SQL sequence/arm mutants: отказ при open, без ремонта; положительный duplicate/next-input control.
- Ready model: scientific spelling математического 100.001 отвергается по grammar,
  обычное 100.001 при tick 0.01 — по off-tick; state/marks/revision не меняются.
- Sparse parsing/accumulation: точные reviewer примеры не проходят; после отказа
  reopen подтверждает прежний frontier. Для двух 4e27+.1 parsing проходит,
  но неверная сумма без .2 отвергается по aggregate mismatch, не по size/identity.
- Grammar/canonical projection: обычные строки, scale/range boundaries и точная
  проекция проверенного значения в f64.
- Accumulation primitives: 0.1+0.2=0.3, сохранение дробной части у больших
  volumes, exact MAX и явный отказ для MAX+10^-9.

Sparse positive controls включают две 4e27+.1 с точной суммой 8e27+.2 и две
8e28+.1 с суммой 1.6e29+.2. Отдельно две точно разбираемые 1e29 не могут
сложиться в допустимый coefficient: это accumulator overflow, не parser failure.
Исходные четыре настоящих SIGKILL frontiers остаются в обеих test profiles.

Переносимые reviewer scripts сохранены побайтно:
scripts/reviewer/m10p1_outcome_index_r0.py и m10p2_exact_decimal_r0.py.
Их provenance/checksums находятся в m10p_regression_provenance.json.
Для R1 применяются --expect-fixed и --expect reject соответственно,
обязательно с явным --refresh-implementation. Точные R0 manifests/candles
сохранены отдельно; меняется только implementation identity в новых owned inputs.
Положительные create/apply/reopen controls исключают ложный PASS из-за чужого
fingerprint или unready seed. Отказы decimal проверяются до durable изменений.

Полные gates: standalone debug/release tests, strict clippy/fmt, 303→606 local
verifier, прежние numerical differential 52/4148 и extra RI 2304, оба reviewer
repro в fixed/reject режимах. Новая широкая V6 CI, VPS, market soak, orders,
WS квоты и естественные сделки не являются gates этой локальной коррекции.

Implementation fingerprint меняется штатно из source/dependencies; исходные
R0 journals автоматически не мигрируются и committed evidence не переписываются.
Неблокирующее замечание о raw millisecond clock остаётся будущему producer bridge:
оно не превращается в третий finding или новый R1 gate.

После SOURCE ACCEPT остаются read-only FINAM mapping USDRUBF/RI, trusted producer
bridge, разделение replay/observed и continuous-session bridge. Каталог сам по
себе не даёт разрешения запуска. Real orders/DB0/runtime-live остаются закрыты.
