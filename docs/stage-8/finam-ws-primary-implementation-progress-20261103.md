# WS primary v1 — текущая реализация, 03.11.2026

Рабочее ТЗ: `finam-ws-primary-review-and-tz-20261102-r0.md`.
Принятый predecessor tree: `73ac00b63b792d3e812b70846c64c7788e55190a`.
Это незавершённый implementation checkpoint, не новый accepted baseline.
Последний срез: B11 — T40 historical CLI fixture / immutable package reproduction.
B10 добавил T39 public export / cleanup faults.
B9 добавил T38 static shared feed / runtime cohort.
B8 добавил T37 admitted-primary replay / frozen Python parity.
B7 добавил source-bound startup recovery и scenario map.
B6 добавил explicit v2 CLI/entry и terminal evidence export.
B5 добавил async semantic EOD/STOP/drain и file-only verifier.
B4 добавил независимый calendar admission/entry и SESSION_END source proof;
B3 — committed-prefix restart/reconstruction,
B2 — bounded reconnect/resync через async runner и actual model writer.
Описания предыдущих срезов ниже фиксируют историю; прежние ограничения заменены
соответствующими последующими разделами. Operational/full-session EOD verdict
пока НЕ открыт: локальный semantic completion не заменяет native/cgroup evidence.

## Выполненный начальный срез итерации A

- Отдельный v2 config envelope с явными `ws_primary_v1`, policy и числовыми
  bounds R0; legacy parser его отвергает. Config/semantic hashes связывают
  policy и все настройки; model/reference/feedback hashes не переписываются.
- Wire normalization сохраняет Missing/ExplicitFalse/ExplicitTrue, проверяет
  aliases, channel/key/symbol/timeframe, timestamp/tick/OHLCV. Обе формы payload
  поддержаны; numeric mapping не выставляет source finality.
- Missing closed bootstrap (включая недавний хвост) допускается только по точным M1 из существующей
  AdmittedClosedM1Snapshot. Source hashes сохраняются; M10 seed не считается
  M1 reference. History не двигает cache/readiness/model.
- Mixed-frame изменения атомарны. Bootstrap512/256KiB, cache4096; duplicate
  не складывает volume и не продвигает DATA. Необъяснимая revision вызывает Hold.
- Source-neutral Decimal aggregation выделена в broker-core; старый REST
  receipt переиспользует её и отдельно сохраняет свою HistoricalPoll authority.
- Offline dense finalizer: десять M1, текущий generation, coverage с начала
  bucket, boundary DATA и grace1000ms; сохранены open label и canonical digest.
  Partial/sparse/no-boundary — PendingProof с ожиданием до C+3000ms, не WS_LIVE.
- Capability AdmittedCanonicalM10 не Clone/Deserialize, admission не двигает
  frontier. Отдельное подтверждение публикации проверяет ownership/clock/Lmax.
  Связь с actual model/feedback выполнена следующим срезом ниже; crash/restart
  qualification полного процесса и конечный verifier всё ещё не закрыты.
- Nonblocking ingress128: overflow/closed/age>2000ms дают continuity latch;
  STOP прерывает ожидание. Socket offer и first-fault slot подключены третьим
  срезом; общий recovery dispatcher ещё не выполнен.
- Gap latch синхронен; новый generation/fresh DATA его не очищает. Terminal
  Hold не снимается вызовом reconnect/gap. Recovery-resolution path ещё не готов.

## Второй срез итерации A: actual session и REST roles

- Explicit `ValidatedConfig::from_ws_primary` связывает envelope v2, policy и
  semantic hash с прежним seeded model. Legacy JSON parser остаётся v1-only;
  отдельные guards не допускают v2 в старый REST-driven RunPlan/runner.
- WS и своевременный REST exception идут через один actual model entry:
  сохраняются source receipt, canonical digest, реальный model callback,
  прежний candle-close feedback, publication fence и отдельная commit boundary.
  Оба frontiers продвигаются только после успешного результата всей операции.
  Pending virtual feedback блокирует commit; повтор частично изменённой модели
  запрещён. Это source implementation, не restart/crash acceptance.
- `runner/primary` — single-writer coordinator, а не ещё один обязательный
  REST worker. Dense WS path не планирует market GET. Partial/sparse/boundary
  proof получает одноразовый ticket после C+3000 с deadline по реальному Lmax.
  Ticket counts — scheduling intents, не подмена счётчика фактических HTTP GET.
- Source-admitted sparse proof сохраняет `REST_EXCEPTION`, reason, raw REST
  evidence и bitmap. Late/неполный/пустой proof не становится WS_LIVE.
  SESSION_END и canonical recovery этим методом пока не открываются.
- Если WS завершился во время GET, ответ сравнивается без второго callback.
  Доступный конфликтующий proof проверяется ДО callback. Сравнение committed
  данных проверяет точный M1 inventory, не только итоговый M10/close.
- Audit — отдельный результат, один pending slot, каждый шестой commit,
  не чаще60min; можно выключить. Timeout/Unavailable не меняет model frontier
  и не прекращает следующий WS admission; auth/integrity/unknown не понижаются
  до сетевой недоступности. Async HTTP executor добавлен третьим срезом ниже;
  общий dispatcher задач подключён четвёртым срезом ниже.
- Opt-in auth-компонент использует тот же exact account/readonly/permissions
  validator и mutex single-flight. Завершённый известный network/endpoint failure
  оставляет проверенный existing socket допустимым, но не выдаёт старый grant
  для новых команд. Новая попытка — полная проверка нового grant; внешний abort,
  negative binding, decode/unknown и clock fault остаются недопуском.
- Локальные тесты вызывают настоящий объект модели и fsync writer. Синтетический
  UTC/monotonic clock существует только под `#[cfg(test)]` и не попадает в ELF.
  Cold composition fixtures не являются доказательством четырёхсессионного
  warmup или live/parity. Проверены source/commit write refusals и отсутствие retry.

## Третий срез итерации A: physical transport / bootstrap / HTTP

- Отдельный primary socket adapter использует прежний fixed FINAM endpoint,
  TLS opener, защищённый grant и transport bounds. Непомеченный history не идёт
  через legacy bool-default BarsProtocol. SUBSCRIBE отправляется с актуальным
  grant; его receipt фиксирует фактическое время отправки, но не provider ACK.
- Processor принимает live observations только после неприсваиваемого извне
  physical-subscription receipt. Frames и DATA feedback связаны opaque owner
  и generation; одних переданных timestamp/generation недостаточно. Test-only
  factory для числовых fixtures отсутствует в release build.
- Socket предлагает целые normalized frames в bounded ingress без await.
  Heartbeat остаётся отдельным async polling path. DATA age обновляется только
  после подтверждения processor и по исходным wall/monotonic receive times;
  history, duplicate, Ping/Pong и время получения ACK её не освежают.
- Реальный stream EOF/Close, protocol/queue/cancellation fault сохраняется
  first-only до общей остановки. Пока этот adapter намеренно fail-closed:
  recovery orchestration и отдельные episode latches будут выполнены в B;
  ordinary STOP/drain подключён четвёртым срезом ниже;
  текущий component PASS не доказывает automatic reconnect/resync.
- Bootstrap bridge сохраняет исходные frame/hash/receive stamp. Единственный
  дополнительный GET имеет независимо заданный closed range и общий deadline;
  start обязан поступить из календаря предыдущей реальной raw session, не из
  принятого WS-пакета или эвристики последних5h. Привязка start к полному
  independently validated calendar остаётся задачей общей композиции.
- После proof frame не получает новое время приёма. Если прошло>2000ms,
  допускается лишь discard целиком подтверждённого history. Mixed/live tail
  вызывает continuity fault без частичного cache/readiness/model effect.
  Ошибка proof закрывает owner; второй GET этим bridge не выдаётся.
- Аддитивный typed GET использует прежний fixed-origin client, без redirect,
  retry, endpoint override или account fallback. Сохраняются реальные raw/hash,
  request/receive times и transport-complete. 401/403, transient unavailable,
  corrupt body и unknown различаются. Счётчик send attempts фиксируется внутри
  client, а не равняется числу scheduling tickets.
- Async HTTP executor связывает exact account, новый auth grant, одночастный
  план и STOP/общий<=5s deadline; auth mutex wait включён в тот же бюджет.
  Внешняя отмена/неопределённый grant не превращаются в безопасный audit timeout.
  Истечение JWT после SUBSCRIBE не закрывает уже существующий socket само по себе.
- Component tests используют in-memory WS peers и loopback HTTP server:
  bootstrap300 Missing rows, atomic rejection последней строки, overflow,
  heartbeat при остановленном consumer, physical-owner fencing, JWT expiry,
  stop/abort, independent proof range, задержанный mixed frame, HTTP auth/status/
  truncated body и auth single-flight budget. Это не FINAM/operational smoke.
  Для HTTP test peer добавлена только dev-dependency tokio, без обновления
  версий locked crates и без новой production dependency broker-finam.

## Четвёртый срез итерации A: общий async runner

- `runner/primary/live` связывает fixed socket, physical SUBSCRIBE receipt,
  ingress, bootstrap proof, отдельные HTTP roles и actual canonical coordinator.
  Production run получает STOP/deadline/policy из того же owned session; accounts
  exact-bound существующим auth/client. Публичных endpoint/transport overrides нет;
  private local-peer/stub seams компилируются только для tests.
- У модели и JSONL один blocking-worker с mailbox1. Он не ждёт network IO;
  socket heartbeat и HTTP продолжают polling на async runtime, пока модель/fsync
  заняты. Command reply передаётся только после всей операции, включая post-write
  fence, feedback и продвижение processor frontier.
- Во время bootstrap pending dequeue остановлен, но физический socket и его
  bounded ingress работают. После proof и history-discard queued live frames
  по-прежнему проверяются по исходному receive time и пределу2000ms. Receipt
  остаётся исходным; время ожидания не скрывается.
- Одновременно не более одного mandatory HTTP job и одного audit; coordinator
  выдаёт одноразовые slots. HTTP results попадают к тому же single writer.
  AuditUnavailable не блокирует следующие WS model commits. Негативная auth,
  неизвестная внешняя cancellation и data conflict сохраняют исходную причину
  до STOP; более общий Held/runner-stopped не подменяет её.
- Реальные HTTP send-attempt/complete-body bytes и elapsed из executor разнесены
  по roles. При отмене без завершённого receipt счётчик отмечается unknown,
  а не выдаётся за доказанный нулевой GET. Полная telemetry auth/account calls
  и независимая terminal cross-validation остаются частью B.
- STOP отменяет IO и ждёт socket, HTTP и blocking writer в одном stop-budget
  из config (не более15s). Writer panic/drop/abort закрывают session. Если
  синхронный callback/fsync не завершился, source runner не может его forcibly
  preempt: drain=false/INCOMPLETE, не clean EOD. Process-level hard-stop obligation
  не заменяется отменой JoinHandle. При external future abort RAII ставит STOP,
  abort async workers и закрывает mailbox; это не квалификация штатного STOP.
- Qualification:11 integration tests, actual WS codec/local peer, настоящая
  cold model и fsync writer, typed admitted REST fixtures и test-only clocks.
  Два dense M10 без GET; восемь commits с audit failure и двумя следующими M10;
  bootstrap exact proof; stale queued tail; STOP во время HTTP; original auth/
  Close cause; external abort; Ping/Pong при dummy-blocked writer; неподтверждённый
  drain после budget. Pause/dummy clock не выдаются за реальный FINAM latency/soak.
- Проверка прогресса тестов намеренно ждёт writer response, а не одну видимую
  canonical_commit строку: STOP между fsync и последним fence остаётся незавершённой
  операцией. Существование строки само по себе не доказывает завершённый callback.
- На этой стадии любой terminal остаётся INCOMPLETE primary bounded stop, а
  RunReport.full_session_qualified=false. Итерация B ещё должна разрешить точный
  calendar/EOD/recovery path. Ни CLI, ни operational activation этим не открыты.

## Первый срез итерации B: recovery policy и source-bound repair

- Добавлен отдельный bounded episode controller:3 attempts, backoff1/2/4s,
  общий45s с меньшим run deadline, до3 episodes/run. Original typed fault и
  canonical latch фиксируются синхронно до первого await. Нельзя маскировать
  дополнительные Connect/REST-команды повтором того же stage внутри attempt.
  Отказ в четвёртом episode оставляет canonical gap/STOP, не healthy readiness.
- Две readiness-защёлки раздельны. Handshake/history/Pong не дают transport
  success. Нужен physical SUBSCRIBE receipt и принятая processor свежая DATA
  того же owner/generation в first-DATA budget. Canonical latch снимается только
  отдельным неприсваиваемым receipt завершённого source-bound repair. Новый
  fault аннулирует старый receipt даже на том же physical connection.
- `RecoveryPlan` связывает текущий gap nonce, config/policy, generation,
  последний committed frontier и независимый cutover=floor(actual now/M10).
  По умолчанию2 overlap labels (либо доступный committed prefix), максимум6h
  пропуска внутри одной model session, один pending одночастный REST plan.
  Нельзя укоротить cutover, чтобы скрыть уже закрытые required M10.
- Все overlap M1 inventory/OHLCV и ВСЕ новые buckets проверяются до первого
  historical callback. Несовпадение committed data, пустой required bucket,
  grid hole, чужой generation/processor/plan или неподтверждённый ответ не
  становятся модельным input. `RecoveryInput` не Clone/Deserialize и не LIVE.
- Coordinator/session продолжают тот же объект модели и virtual book. Новый
  gap проигрывается последовательно через существующий HistorySim/candle-close
  feedback, с origin `RECOVERY_BACKFILL`, mode `BACKFILL_REPLAY`. Переключение
  model delivery context не создаёт новую модель и не обнуляет position/side-used.
- Номинальный close-clock у historical model/fill не используется как подмена
  UTC transport/времени исполнения. Реальные request/receive/replay/commit times
  сохраняются отдельно. Actual wall/monotonic fence, STOP, run deadline и
  bounded replay budget проверяются и после fsync. Повтор partial callback
  после ошибки записи/commit запрещён; gap остаётся unresolved.
- Raw REST source сохраняется один раз на batch, а каждый canonical source
  ссылается на него и хранит final M1/bitmap/digest. Overlap не вызывает модель.
  Fresh DATA не снимает gap ни до, ни после replay без explicit resolution.
- Исправлен edge case forming M1 при новом generation: такое же значение
  нового физического потока заменяет old-generation provisional version и
  подтверждает DATA нового generation, но не закрывает canonical gap. Дубликаты
  внутри одного generation по-прежнему не освежают DATA и не суммируют volume.
- Qualification этого среза: data-core негативные случаи, actual coordinator/
  fsync replay двух missed M10 после двух overlap, source/commit write failures,
  two-latch policy/budget/cancellation и перенос реальных long/short virtual books,
  полученных из frozen fixture прежним model host. Проверка long/short — именно
  test instrumentation; не импорт production state и не новая parity/session
  acceptance. Новая helper-функция в model.rs существует только под cfg(test).
- В tests исправлена гонка paused timer у dummy stalled-writer проверки;
  release-guard освобождает dummy thread и при assertion failure. Session guards
  используют тот же clock helper, что callback fence: в production это actual UTC,
  override существует только в cfg(test). Провайдерские timestamps не меняются.

На момент B1 это был компонентный checkpoint, ещё без async reconnect. Подключение
controller и replay выполнено следующим срезом B2; global STOP не очищается.
Для первого bucket без committed overlap и закрытых missed M10 нет proof-less
resolution: пока HOLD; accepted historical-prefix entry для этого случая нужно
связать при общей композиции. Отдельного request/approval на operational запуск нет.

## Второй срез итерации B: reconnect/resync через async runner

- Разделены независимый operator/run STOP и local cancellation конкретного
  соединения. Close/EOF закрывает синхронный continuity guard до первого await,
  но не сбрасывает модель и не останавливает весь run без попытки bounded recovery.
  Старые socket/HTTP tasks отменяются и дренируются до следующего поколения.
  Новый физический socket не может очистить run STOP или fault нового поколения.
- Writer и live callback/publication fences читают тот же guard. Пока gap не
  разрешён, нет обычных model callbacks, ticks и audit scheduling. Если fault
  попал внутрь изменения модели или durable publication, результат остаётся
  INCOMPLETE: частично выполненный callback не повторяется с новым временем.
  Historical reconstruction использует отдельную source-bound capability, а
  не отключение live fence или подмену фактических transport timestamps.
- Каждый attempt создаёт свежую auth-команду и physical SUBSCRIBE. Production
  factory использует существующий exact account/readonly/permissions validator,
  реальные expiry и auth/connect/subscribe budgets внутри общего episode budget.
  Только принятая DATA текущего owner/generation закрывает transport latch;
  history, server Ping и Pong недостаточны. Тот же writer затем проверяет весь
  REST overlap, выполняет historical commits и явно разрешает canonical latch.
- Одноразовые bootstrap/recovery slots привязаны к поколению. Завершённый
  Unavailable/Timeout допускает следующую bounded попытку; negative auth,
  повреждённые данные, неизвестная/частичная выдача grant не превращаются в
  сетевой retry. Отмена GET после доказанно завершённого grant отличается от
  неопределённой отмены auth. Ожидаемый Cancelled при operator STOP не создаёт
  фиктивный market fault и не стирает исходный fault.
- Разрешение guard проверяет текущую generation/revision и источник завершённых
  commits. Поздний новый fault аннулирует resolution; malformed DATA и auth
  rejection сохраняют закрытый тип ошибки, а не становятся ReferenceUnproven.
  В journal/RunReport сохранены episode, attempts, исходная причина, обе latch,
  unresolved fault и раздельные HTTP роли; terminal пока всегда INCOMPLETE.
- Сквозной local-peer сценарий проверяет и реальный Close, и abrupt EOF:
  два WS commits → разрыв → новое соединение → один RECOVERY_BACKFILL commit →
  один REST_EXCEPTION для первого неполного bucket нового поколения → следующий
  полный WS_LIVE. Модель и writer остаются прежними; overlap не вызывает callback.
  Первый partial bucket не выдаётся за непрерывно наблюдавшийся WS interval.
- Восемь новых async tests покрывают этот сценарий, STOP во время backoff и auth,
  завершённый auth Unavailable с новой попыткой, corrupt DATA без network retry,
  REST overlap conflict до historical callback и history/Ping без fresh DATA.
  Повторный Close после fresh DATA, пока REST resync ещё выполняется, снова
  закрепляет transport_unresolved и canonical_unresolved, даже при гонке с STOP.
  Исходный fault сохраняется; завершённый прошлый episode не переписывается.
  Дополнительно проверены synchronous guard, fault внутри callback/publication,
  expected STOP cancellation и поздняя non-retryable ошибка при resolution.
  Старые11 Stage-A integration tests сохранены как regressions; managed path
  проверяется отдельно. Clocks/grant factories и REST receipts здесь тестовые;
  это не provider rotation, TLS/FINAM smoke, natural recovery или operational soak.

## Третий срез итерации B: committed prefix и restart

- Отдельный restart admission связывает точные bytes/hash predecessor JSONL,
  config/policy/model/reference/feedback/seed, previous segment и независимо
  переданный required inventory. JSONL ограничен по bytes/rows, требует полного
  newline, последовательных sequence/labels и согласованной attribution каждой
  строки. Duplicate JSON keys отвергаются до Value/last-wins преобразования.
- Parser независимо пересчитывает Decimal M1→M10, bitmap/digest и typed source
  hashes. WS receipts, inline REST_EXCEPTION и retained RECOVERY_BACKFILL
  различаются; REST raw bytes/times повторно проходят прежний admission.
  Проверяются source→step→необходимый fill→publication→commit, provenance,
  state chain, временные bounds и отсутствие повторных эффектов. Один boolean
  feedback_complete не закрывает пропущенный fill или частичный tail.
- `CommittedPrefix` не Clone/Deserialize. Неполный source/model/fill tail остаётся
  HOLD, без truncation старого файла и без автоматического retry callback.
  Полный source admission предшествует созданию нового output. Незавершённая
  реконструкция нового сегмента тоже не принимается как произвольно укороченный
  prefix: проверяется заявленный reconstruction anchor/count.
- Новый source entry повторно строит модель из исходного четырёхсессионного seed,
  проигрывает retained canonical M1 через прежние candle callbacks/full fills и
  буквально сравнивает decisions, observations, sequence/action/quantity, price,
  position/average с pinned prefix. Никакого model-state setter/deserialize нет.
  Согласованно изменённые saved observations не подменяют реальный расчёт модели.
- Для точного restart сохраняются записанные callback/result/fill clocks как
  явно исторический виртуальный clock: pending request ID зависит от callback
  timestamp. Подмена его номинальным close меняла ID; ID не вырезается из сравнения.
  Реальные reconstruction/result/fsync times и UTC/monotonic/STOP/10s budget
  проверяются отдельно, через тот же production clock helper без override в ELF.
  Live CallbackFence не ослабляется и не вызывается с историческим временем.
- Новый journal пишет `RESTART_BACKFILL`, source M1, связь с предыдущими journal/
  source hashes, исторические steps/fills и новые commits. Повторный restart
  поддержан. Predecessor journals сохраняются для полной исходной lineage;
  новый файл сам по себе не аутентифицирует FINAM или старую WS delivery.
- В processor возвращаются только committed M1/frontier. Provisional cache,
  physical owner, DATA age/readiness и старый socket не импортируются; состояние
  Bootstrap, отдельный restart_committed counter. Продолжение связано через
  обычный Coordinator; overlap не вызывает повторный callback. Проверена цепочка
  prefix19 → один historical recovery с overlap2 → REST_EXCEPTION текущего
  partial bucket → следующий restart того же source-bound журнала.
- Qualification: синтетический реальный четырёхсессионный seed, long и short,
  повторная реконструкция, hash/identity/order/numeric/state/clock mutations,
  запись в существующий файл и STOP. На Unix дочерние процессы действительно
  получают SIGKILL в пяти точках: до source, после source, step, fill и commit.
  После kill читается реальный fsync JSONL, old bytes не меняются; partial pending
  не продолжается, а complete prefix сохраняет flat/nonflat состояние.
  Pause/marker hook существует только в cfg(test), не входит в release.
- Это локальный source checkpoint, не automatic process-supervisor restart,
  FINAM/ALOR parity или full-session qualification. Публичный ws_primary CLI
  по-прежнему закрыт до календарной/EOD/общей qualification. Restart admission
  использует внешний trusted binding/inventory; это не новая система его выдачи.

## Четвёртый срез итерации B: independent calendar entry / SESSION_END

- Отдельные pinned inputs: ModelDay, raw FINAM calendar с неизменёнными GET body/
  hash/request/receive times и прежний mapping. CalendarBinding связывает их с
  config, полным required inventory, entry window, next label, EOD cutoff и
  явно заданным bootstrap reference start. Календарь не строится из полученных
  M1/первых300 rows/выбранных непустых buckets.
- Для baseline07 R0 проверяется полный обычный weekday:101 labels от07:00 до23:40,
  close23:50. Raw schedule проверяется отдельно: поддержанные trading/auction/
  closed интервалы, непротиворечивость, известное покрытие текущего model window
  и наличие торговой части у каждого required bucket. Short/closed/unknown
  сессия — HOLD до открытия transport, не придуманная M10 или бесконечный retry.
  Автоматическое изменение модельного календаря под shortened day не добавлено.
- Previous raw-session date может быть выходным, независимо от model warmup.
  Проверяется её фактическое окно, известный промежуток до model open и отсутствие
  пропущенной более поздней raw session. Bootstrap range допускается шире этой
  даты в пределах pinned seed range: тихий предыдущий день не гарантирует300 M1.
  Start закреплён ДО WS; нельзя укоротить его под пришедший frame. Существующие
  GET/cache/bootstrap budgets и exact-M1 comparison остаются обязательными.
- Current schedule должен иметь известное покрытие из snapshots не старше24h
  при entry (это client policy, не гарантия FINAM). Исторические snapshots
  предыдущей raw session сохраняют исходные времена и не retimestamped.
  Duplicate JSON keys и unknown raw fields отвергаются до public evidence.
- Production library run теперь требует AdmittedCalendar; свободный reference
  timestamp больше не является operational entry. До socket task проверяются
  calendar window/reference и соответствие processor/session frontier. Допуск
  первого physical SUBSCRIBE receipt проверяет sent_at и текущее время внутри
  entry window: поздний receipt не открывает модель. Последующий reconnect
  не начинает run заново, но также не очищает gap/DATA readiness.
  Пропущенный intraday prefix нельзя заменить новой нулевой моделью.
- Для последнего calendar-bound label всегда нужен один SESSION_END proof
  после C+3000ms, даже при полном WS M1 inventory. Следующая M1 не требуется.
  Origin остаётся REST_EXCEPTION с reason=SessionEnd, не WS_LIVE. Deadlines
  ограничены proof/run/EOD/Lmax bounds; отдельный HTTP role/count SessionEnd.
  Sparse математика, conflict checks, fresh DATA и actual callback/fsync fence
  остаются прежними; неверный reason, early/late/unbound/no-DATA/conflict — отказ.
- Локальный actual warmed model/source flow прошёл100 WS M10 + один last proof
  без next-M1:101 commits, один scheduled mandatory proof и ни одного обычного
  per-M10 proof ticket. Этот file-only flow не измеряет сетевые GET.
  Дополнительно проверены raw weekend vs model day, stale/short/hole/conflicting
  schedule, mapping/model inventory, duplicate/unknown JSON, missed prefix,
  late first SUBSCRIBE, continuation generation и отказ до connection factory.
  Это source/component qualification; full async EOD и operational natural
  session здесь не заявлены. CLI по-прежнему закрыт.
- Исправлен найденный B3 edge case: retained REST evidence при restart теперь
  повторно проверяется AS OF исходного collection time, а не объявляется свежей
  спустя15min. Все исходные bytes/hash/times сохраняются, future receipt всё ещё
  запрещён; helper приватный и отдаёт только historical prefix capability.
  Reproduction с recovery + inline REST_EXCEPTION старше15min прошёл; live REST
  admission/freshness не изменены.

## Пятый срез итерации B: async semantic EOD / drain / file-only verification

- Bound model calendar задаёт плановый EOD trigger; run/recovery deadlines не
  продлевают его. После trigger новые callbacks не планируются. STOP, socket,
  HTTP и blocking writer используют один бюджет не более15s; опоздание event
  loop входит в тот же бюджет. UTC и monotonic duration сверяются с config
  uncertainty; clock jump/late drain не дают semantic PASS.
- Finalizer требует весь independently admitted inventory, committed SESSION_END,
  flat model, отсутствие pending feedback/bootstrap/inflight и unresolved
  transport/canonical/recovery. 101 строка или успешный callback сами по себе
  недостаточны. Незавершённые операции и operator STOP остаются INCOMPLETE.
- На EOD отменяются HTTP/socket; известный завершённый audit сравнивается, а
  известная отмена отмечается CancelledForStop. Неизвестная отмена, auth/security
  failure, conflict и worker panic не становятся AuditUnavailable. Поздний
  отрицательный socket result сохраняет исходную причину даже во время STOP.
- Writer сохраняет лишь `EOD_PRIMARY_PREPARED_NOT_DRAIN_ATTESTATION`, затем
  fsync и закрытый journal seal. Он НЕ удостоверяет собственный join. Parent
  получает отдельный Completion только после ожидания socket/HTTP/writer;
  поле semantic_eod_passed проверяет совокупность условий. Completion — read-only
  diagnostic DTO, не новая runtime/restart authority. Пока возвращается как
  значение library API; запись отдельного файла остаётся задачей CLI/export.
- File-only verifier принимает внешний exact byte binding config/calendar/
  journal/completion. Повторно проверяет сохранённые исходные calendar JSON при
  исходном admission time, identity/sequence, M1→M10 Decimal math, typed source
  hashes, mode/origin, callback/fill/book arithmetic, publication/Lmax, complete
  commits, SESSION_END, unresolved latches и post-join receipt/budget. Match audit
  заново сравнивается с сохранённым raw source и точным committed M1 inventory;
  Unavailable разрешён отдельно и не означает parity. Duplicate JSON keys,
  partial tail и неверный terminal отвергаются. Model oracle остаётся отдельным
  T37: verifier не импортирует model state и не заменяет полный strategy replay.
- Пять новых integration tests: actual warmed model, физический local WS codec,
  fsync journal,100 WS +1 SESSION_END, без следующей M1; Match/Unavailable audit;
  operator STOP; rebound receipt/journal mutations (hashes обновлены, но нарушены
  completeness/origin/clock/source/latches). Подтверждён один обязательный
  SessionEnd GET attempt в stub executor, ноль ordinary per-M10 proof. Это
  compressed-day synthetic clocks, НЕ реальная17h сессия, FINAM или auth rotation.
- `full_session_qualified`, network/cgroup/parity остаются false; verifier также
  не принимает заявление этих квалификаций в library Completion. Native hard-stop,
  процессный witness и operational export qualification остаются отдельными
  доказательствами; в этом срезе VPS/установка не выполнялись.

## Шестой срез итерации B: explicit entry / CLI / post-join export

- Добавлен `primary::entry::Plan` с exact config/calendar binding, account и
  readonly/auth policy, explicit fresh/committed-prefix startup. Preparation
  проверяет все public bytes/seed/calendar/prefix ДО output/auth/network.
  Fresh с previous segment или пропущенным intraday prefix запрещён. Restart
  делегирует принятой deterministic source reconstruction B3, сверяет next label
  с независимым календарём; old output не меняется, DATA readiness не переносится.
- Отдельные v2 команды observe/validate-ws-primary и варианты `-restart`;
  `verify-ws-primary` вызывает file-only verifier. Legacy parser/runner всё ещё
  отказывают v2, автоматического fallback нет. Validate не читает credential и
  не создаёт auth/client/output; receipt различает preparation и open window.
  Observe проверяет окно до чтения credential; entry/open и physical SUBSCRIBE
  перепроверяют actual time/freshness. Endpoint, broker execution или quote
  overrides отсутствуют. Account у plan/auth/client должен совпасть до output.
- Credential остаётся explicit protected file по прежним permissions, без env
  discovery; public entry receipt содержит plan hash, но не account/secret/path.
  SIGINT/SIGTERM ставят общий STOP и дожидаются runner future/drain/export, не
  бросают выполняющийся callback. Процессный watchdog по-прежнему отдельный.
- Новый output directory create-only, Unix0700; файлы0600/create_new/fsync.
  После join сохраняются RunReport, Completion, exact EvidenceBinding и результат
  повторной file-only проверки; manifest пишется последним, затем fsync каталога.
  Report и Completion не могут противоречить drained/count/cause facts для PASS.
  При недоказанном join — только diagnostic export-incomplete, без manifest и
  pins потенциально меняющегося журнала. Старые файлы/папки/leaf symlinks не
  перезаписываются. Ошибка оставляет partial output для разбора, не retry туда же.
- Пять EOD integration cases B5 теперь проходят через actual PreparedEntry →
  new directory/session → local-peer runner → post-join disk export → verifier.
  Проверяются все manifest hashes, completeness и отказ повторного export.
  Дополнительно шесть entry tests: fresh window/STOP/output, input/mode/security
  refusals, admitted prefix2→same frontier, partial prefix, wrong auth/client
  account, unresolved worker без manifest. Две executable tests проверяют full
  synthetic four-session inputs через file-only CLI, отсутствие account в stdout,
  ранний отказ observe вне окна, old config/restart mode и fake completion.
- CLI существует только в candidate source; нового release/установки/активации
  в этом срезе нет. Полная source matrix и operational/native qualification не
  объявляются. Original inputs и predecessor journal остаются нужными для lineage.
  Точный CLI/output contract: `finam-ws-primary-entry-and-export-20261103.md`.

## Седьмой срез итерации B: startup recovery / source-linked verifier / matrix

- Закрыт zero-model-gap startup: до первого committed/missed M10 по-прежнему
  нет пустого proof-less resolution. Только при calendar-bound model open
  recovery plan запрашивает fixed reference start→model open. Требуется новый
  nonempty source-admitted closed-history snapshot в прежнем5s budget;
  известные M1 сверяются, cache4096/depth/one-part guards не меняются.
  История не даёт DATA readiness, callbacks или продвижения frontier.
- Existing opaque completion потребляется обычным resolve после нового physical
  SUBSCRIBE, bootstrap и admitted fresh DATA текущего generation. Scope явно
  журналируется STARTUP_ANCHOR, actual request/receive/deadline сохраняются.
  Первый partial bucket всё ещё требует REST_EXCEPTION после C+3000; anchor
  не выдаётся за WS_LIVE. Обычный model gap сохраняет overlap2 и предел6h.
- Verifier связывает source-validated event с сохранённым raw snapshot и затем
  с canonical-resolved/episode-ready. Для MODEL_GAP независимо проверяет range,
  counts и точные M1 всех overlap/missing labels; для STARTUP_ANCHOR — calendar
  bounds, generation, nonempty source, отсутствие model steps и реальные времена.
  Подмена scope, удаление validation с перенумерацией строк и rehash, неверные
  bounds/counts не дают semantic EOD. Schema-compatible old regular-gap event
  может не иметь нового generation field; неправильный тип не coercion.
- Четыре component tests: bound source обязателен даже после DATA, no-DATA и
  второй fault не принимают старый completion, late/conflicting/empty/early/wrong-
  range proof не двигает frontier. Full warmed-model local-peer EOD проверен
  с Close до первого DATA и после первого provisional DATA:99 WS +2 REST
  exception,0 historical model callbacks от startup anchor,101 commits.
  Отдельный EOD с overlap2/gap1:98 WS +2 REST +1 recovery label; verifier повторно
  проверяет proof и отвергает подмену overlap count. Это synthetic day, не soak.
- Managed-driver test удерживает critical proof pending: boundary DATA + grace
  позволяют ровно один WS commit; поздний proof становится сравнением без второго
  callback. Альтернативно STOP во время этого GET возвращает известную отмену,
  все workers drained,0 callbacks, не EOD. Grace в тесте отсчитывается от actual
  boundary receipt, а не от nominal close — production rule не ослаблено.
- Добавлен `finam-ws-primary-acceptance-map-20261103.json`, все40 ID сохранены.
  `matrix.py` связывает конкретные fn/source hashes с реально прошедшими строками
  regression log и общим source pin. LOCAL_EVIDENCE — не ACCEPT и не обещание
  operational полноты.36 строк имеют local evidence; T37/T39/T40 остаются PARTIAL,
  T38 OPEN. Генерируемый evidence — рядом с verification.json, не в source.

## Восьмой срез итерации B: T37 historical admission / model parity

- Одни и те же505 исторических M10 (21–25 сентября) пропущены через новый
  `PrimaryProcessor::from_config` → admitted source → actual `ObservedSession`
  → модель → candle close-fill → fsync journal → commit. Четыре исходные seed
  сессии /404 M10 прогревают модель один раз; между пятью днями нет повторного
  seed, setter/import состояния или сброса модели/виртуальной книги.
- Три варианта одинакового числового входа: dense WS (500 WS_LIVE +5
  BoundaryUnproven REST_EXCEPTION), все505 через REST_EXCEPTION и все505 через
  SparseUnproven proof. Последняя M10 каждого дня без следующей M1 в этом
  компонентном тесте честно REST_EXCEPTION, не WS_LIVE и не SESSION_END.
  Production calendar/SESSION_END проверяются отдельно B4–B7.
- M1 здесь **синтетически разложены из закреплённых M10**, с точным сохранением
  Decimal OHLCV: весь high/low/volume во второй минуте, остальные численно
  нейтральны. В sparse варианте отсутствует минута с offset=4 (bitmap1007),
  отсутствующая M1 не создаётся. Это не реальные FINAM M1 и не доказательство
  их плотности, наблюдения сети или intraminute price path.
- Одна actual model session и пять daily numeric admission owners; private
  cfg(test) identity/clock seams. Тест не проходит production multi-day calendar
  entry/CLI, не квалифицирует полную EOD сессию и не расширяет production bounds.
  `ObservedSession::create` использует общий UTC helper `timing::now_ms`, как и
  callback fence: в production это прежний actual UTC, фикстурные часы существуют
  только в test build. Параметры стратегии и feedback не изменены.
- Для каждого label сверяются сохранённые source/M1/canonical digest с исходным
  CSV, before→after continuity, решения, branch/no-intent reasons/veto, side-used
  до feedback, все frozen индикаторы/anchor/day/position/used fields после
  feedback, request sequence и close-fill цена/позиция/average. Десять fills
  образуют пять прежних round trips (2 EOD и3 short stop1 exits). Итоговая semantic
  проекция использует actual значения, а не подставленный oracle; clocks/source
  origins исключены из сравнения трёх admission режимов. Числовой tolerance1e-10
  совпадает с прежним state parity тестом; Decimal OHLCV/price ticks точны.
- Отдельный test-only reader отвергает10 mutations: numeric/reason/state/fill/
  commit/duplicate/seed/policy/pending/origin. Frozen CSV, Python oracle и его
  исходники не меняются; исходный независимый Python generator повторно запускается
  с `--check` (505 steps/5 rounds). Старый OfflineReplay тест сохранён как regression,
  но не используется вместо нового primary path.
- Verification сохраняет отдельные journal/config/model-records/result каждого
  режима, их hashes и общий semantic hash в `t37-parity-evidence.json` рядом с
  `verification.json`. Это local historical component evidence, не operational
  ALOR parity / source acceptance / Linux release. Scenario map теперь37
  LOCAL_EVIDENCE, T38 OPEN, T39/T40 PARTIAL; ACCEPT по-прежнему не выставляется.

## Девятый срез итерации B: T38 shared feed / static consumers

- `Coordinator::new_group` принимает статический Vec из1–8 actual
  `ObservedSession`. До binding проверяются одинаковые полный config fingerprint
  (включая policy/model/seed/calendar/run bounds), следующий label и общий root
  STOP channel. Каждый consumer владеет собственной моделью/book и отдельным
  create-only journal. Runtime index фиксирован порядком в группе и записан в
  `primary_runtime_cohort_bound`; путей/секретов в этой записи нет. Empty/oversize,
  другой config/STOP/frontier отвергаются, dynamic attach/replace отсутствует.
- В группе остаётся ОДИН `PrimaryProcessor`, physical socket/ingress, bootstrap,
  recovery, proof/audit scheduler и canonical frontier. Один blocking writer
  последовательно доставляет один и тот же opaque admitted input всем consumers:
  capability не клонируется и не создаётся заново. После каждого actual model /
  feedback/fsync сохраняется общий barrier со всеми consumer commits; только
  после его записи во ВСЕ журналы и повторной проверки исходных timing/STOP/
  continuity fences вызывается единственный `processor.committed`/`recovered`.
  Lmax не перезапускается после первого consumer; replay остаётся в исходном
  actual deadline и не подменяет clocks/WS readiness.
- Ошибка callback/source/feedback/commit/barrier — terminal для общей операции:
  root STOP, common frontier не двигается, очередного consumer или повторного
  callback нет. Уже изменённые модели и локальные commits не откатываются и не
  называются общей успешной публикацией. Диагностические writes/finish посещают
  остальные members даже после отказа одного журнала. Panic также оставляет
  coordinator failed; async writer сохраняет прежний catch/join/STOP путь.
- Сохранён старый single-runtime constructor и CLI. Группа — ограниченная
  library composition с тем же профилем, не новый multi-profile CLI, execution
  dispatcher или operational deployment. Для текущей одной BO-only модели не
  требуется новый aggregate multi-journal EOD/restart протокол: группа намеренно
  НЕ получает single-journal EOD PASS/seal, а отдельный member journal со своим
  local commit отвергается старым prefix/EOD reader по cohort marker. Это
  предотвращает принятие частичного fanout за общий committed frontier. Existing
  single-runtime EOD/restart semantics не меняются; aggregate cohort qualification
  и dynamic membership не заявляются.
- Четыре targeted unit tests: три consumers/7 labels, duplicate frames/ticket,
  единственные proof/audit slots; source/commit refusal во втором consumer;
  common barrier Lmax/STOP после всех callbacks; invalid config/STOP/frontier/size.
  Новый frozen-corpus test проводит три независимых модели через первые48 M10:
  short2252 → stop2262, два close fills у каждой модели и все frozen states.
  Отказ feedback fsync во втором consumer на entry оставляет общий frontier33,
  честные partial journals и отсутствие повторных fills/callbacks. Состояния не
  импортируются и не задаются setters; несовпадающий frontier для negative case
  также получен реальным callback.
- Два managed async tests используют настоящий local WS codec, existing
  reconnect driver и stub HTTP executor, НЕ FINAM: (1) три models/7 labels,
  одна socket factory call, один proof и один audit; (2) принудительный Close,
  два physical generations, ОДИН gap-recovery GET role +ОДИН partial proof на
  всю группу и origins WS/WS/RECOVERY_BACKFILL/REST_EXCEPTION/WS у каждого.
  GET attempts здесь принадлежат stub receipt, не квалифицируют реальный FINAM
  транспорт. STOP завершается с join всех workers, но не EOD PASS.
- Verification сохраняет все три public consumer journals/config/result этих
  двух async сценариев и их hashes в `t38-shared-feed-evidence.json`; отдельный
  gate связывает fault/feedback tests с source pin. Mapping имеет38 строк
  LOCAL_EVIDENCE; T39/T40 остаются PARTIAL. Это inventory локальных доказательств,
  не independent acceptance и не aggregate operational/multi-runtime readiness.

## Десятый срез итерации B: T39 public export / cleanup faults

- У export обнаружен и закрыт крайний случай: `manifest.json` уже создан,
  но его file fsync или последующий directory fsync не прошёл. Теперь попытка
  имеет однонаправленные состояния New/Admitted/Finalizing/Finished/Failed.
  Refused admission не запускает runtime; ни failed attempt, ни уже завершённый
  export не повторяются и не перезаписываются. Второй finish после успешного
  первого НЕ отзывает ранее принятый manifest и вообще не выполняет новый IO.
- При export error отзывается только `manifest.json`, который сама текущая
  попытка действительно создала через create_new. Foreign file/leaf symlink не
  удаляются. Journal, entry/source/completion evidence и partial public files
  сохраняются. Borrowed external credential не создаётся и НЕ unlink-ится:
  source entry не владеет его lifecycle, и новый secret manager не добавляется.
- Unlink → directory fsync отзыва → best-effort `export-failure.json` → final
  directory fsync выполняются независимо. Ошибка любого действия не пропускает
  остальные. Receipt содержит только фиксированный public file/phase и IO action,
  а не raw OS error/path/account/credential. При любой первичной ошибке return
  остаётся Err, export/semantic PASS не выдаётся. Сам failure receipt тоже может
  оказаться недоступным/недолговечным — он не является durability witness.
- Если unlink отказал, manifest может физически остаться; если одновременно
  отказала запись failure receipt, отсутствует и надёжная публичная диагностика.
  Это честный UNKNOWN, не успешный пакет. Manifest явно требует successful export
  return; отдельно лежащий файл не доказывает export/durability/worker completion.
  Не принимать такой каталог по manifest в обход неуспешного exit/receipt. File-only
  semantic verifier по-прежнему проверяет journal/Completion, а не приёмку export.
- Private per-instance cfg(test) IO seams проверяют open/write/file-fsync каждого
  public JSON, bounded read, final directory-fsync, unlink и отказы самого cleanup.
  В production это обычные filesystem calls; fault switches/env/endpoint overrides
  отсутствуют. 25 export fault combinations (включая unlink + failure-receipt
  failure) и2 foreign collisions используют actual verified compressed-day EOD,
  а не выдуманный PASS report. Positive control повторно проверяет101 commits,
  100 WS_LIVE +1 SESSION_END REST_EXCEPTION и immutable повторный finish.
- Ещё два tests закрывают3 admission receipt failures и unknown/mutable writer:
  незавершённый journal не читается/не хэшируется/не запечатывается даже при ошибке
  export-incomplete receipt или его directory fsync. Dummy borrowed credential и
  source journal byte-for-byte не меняются; public receipts не содержат canaries.
- STOP fault test прямо использует existing Engine.finish/drain_jobs/Connection/
  Writer: normal cancellation, HTTP panic, missing receipt, stuck HTTP, socket
  panic и terminal-record IO refusal. Счётчики подтверждают посещение critical,
  audit, socket и writer cleanup, даже если предыдущий шаг отказал. Завершение
  после deadline в test harness лишь убирает fixture task/thread; исходный report
  НЕ повышается до drained/PASS. После journal write refusal не создаётся
  синтетический segment_end. Ни один из6 STOP scenarios не объявляется clean EOD.
- Evidence: `t39-cleanup-evidence.json`, public positive fixture +27 export case
  records и6 worker reports рядом с verification.json. Positive fixture имеет
  synthetic reference clock исходного compressed-day теста, НЕ actual-now CLI
  qualification. Для T40 нужен воспроизводимый positive file-only CLI fixture с
  согласованной прошлой synthetic session, без clock override/переметки реальных
  provider snapshots. Systemd/native/cgroup cleanup здесь не выполнялся и не
  подразумевается по IO/unit PASS. Mapping:39 LOCAL_EVIDENCE, T40 PARTIAL.

## Одиннадцатый срез итерации B: T40 immutable source / file-only CLI

- Добавлен отдельный полностью synthetic dataset на 28.09.2026 с четырьмя
  предшествующими synthetic warmup sessions. Он создаётся test-only генератором
  от фиксированной прошлой даты: настоящие FINAM snapshots, frozen505 corpus и
  параметры/feedback модели не перемечаются и не меняются. Dataset содержит
  настоящий model/commit/terminal журнал compressed-day теста,101 labels,
  100 WS_LIVE +1 SESSION_END REST_EXCEPTION и post-join Completion, но НЕ является
  наблюдением FINAM,17h soak, native/cgroup evidence или ALOR operational parity.
- `fixtures/ws-primary-file-only-20260928/fixture.json` закрепляет bytes config,
  binding/journal/completion. Library проверяет их с actual UTC now; отдельный
  integration test запускает обычный executable `verify-ws-primary` (не cfg(test)
  clock и не observe). Проверены101/100/1, закрытые qualification flags и отказы
  при rebound hashes для unjoined/unprepared/claimed-network Completion.
  Read-only fixture не меняется. Validate/legacy/refused-mode regressions сохранены.
- T40 не объявляется выполненным из одного unit PASS: source mapping намеренно
  оставляет PARTIAL до внешнего source-bound receipt. Portable review tool
  проверяет SHA/member inventory/modes/Git tree, распаковывает только regular
  create-new files в новый каталог, повторяет offline build/tests/clippy/oracle
  и запускает положительный file-only CLI без clock/endpoint overrides. Source
  inventory проверяется до/после; build target находится снаружи. Итоговый mapping
  может разрешить T40 только по совпадающему extracted-package receipt.
- Финальный review ZIP содержит полное дерево (включая hidden files, fixtures,
  docs старых этапов, Cargo.lock, scanners/gates), точный diff к predecessor tree,
  manifests, выбранное public evidence и portable tooling. `.git`, target/cache,
  borrowed credentials/private ceremony и логи/архивы из общей working directory
  не копируются. Source identity задаётся точным Git tree OID + SHA-256 inventory
  и source ZIP; это НЕ выдуманный/published commit. Main/branch/index не меняются.
  Старые handoff-commit/evidence в source сохраняются как lineage, не candidate ID.
- Последовательность: полная проверка source → неизменяемый source ZIP → extracted
  reproduction → final external matrix/receipt → внешний review package и safety.
  Final receipt не записывается внутрь source с circular self-hash. Source/release
  acceptance, native/cgroup/FINAM operations и installation из этого не следуют.

## B12 / Source R1 — corrections to the 2026-11-04 HOLD

One bounded correction slice addresses the four reviewed branches without
changing strategy, close-fill, policy thresholds or operational permissions.
See [the R1 correction map](finam-ws-primary-r1-corrections-20261104.md) for exact
semantics and executable positive/negative regression entry points. Candidate
identity, applicable gates and full extracted-source reproduction are sealed
externally. R0 source evidence is retained, not relabelled as R1 acceptance.

## B13 / Source R2 — remaining R3/P2 due/readiness cursor

R1 review closed R1/P1, R2/P1 and R4/P2; source as a whole remained HOLD for
deadline loss when IO advances time inside a writer command. R2 retains each
unassessed due using a per-label readiness cursor, instead of filtering it using
the later Update clock. Managed regressions change the sample inside the command,
cover MATCH/equality/after-due/MISMATCH/Lmax/STOP, and require no new DATA or job
completion. A directly exposed owner-fault retirement panic is also removed
without treating that fault as clean readiness. See the
[R2 correction map](finam-ws-primary-r2-due-cursor-20261104.md).
Exact pins, gates and reproduction remain external, not independent ACCEPT.

## Что не выполнено и не должно подразумеваться по unit PASS

1. Итерация A: ordinary async composition реализована и локально проверена,
   но это не source acceptance всей T01–T22 matrix. B7 добавил managed proof/WS
   race и STOP case; raw-calendar/entry guards сохранены.
   CLI v2 подключён B6, legacy fallback отсутствует; source acceptance ещё нет.
2. Итерация B: async socket/HTTP/writer recovery подключён и проверен на local
   peers. Durable prefix и restart source entry реализованы и проверены B3.
   B4 добавил независимые calendars/entry и SESSION_END source proof; B5 — async
   semantic EOD/STOP/drain и file-only verifier; B6 — entry/CLI и disk export.
   B7 добавил source-bound startup без committed overlap. Остаётся итоговая
   общая qualification: proof-less latch reset по-прежнему запрещён.
3. Закрыть T01–T40 на полном runner/socket/model. Текущий checkpoint проверяет
   числовые/буферные случаи и actual model coordinator, sparse proof, audit
   isolation и auth transitions локально; отдельные WS/HTTP/heartbeat components
   доказаны local peers; сохранены11 Stage-A и добавлены managed integration cases,
   не FINAM network qualification и не полная acceptance matrix.
   Точный остаток — строка T40 mapping, без замены требований count-only PASS:
   T37 historical admitted-path model parity добавлена B8;
   T38 —bounded same-config shared cohort реализован B9; текущий CLI по-прежнему
   single runtime, aggregate cohort EOD/restart qualification не заявляется;
   T39 —public export/cleanup fault coverage реализована B10; native/cgroup
   qualification не заявляется;
   T40 —reproduction из immutable package с positive file-only verifier fixture.
   Operational/native process qualification остаётся отдельной, не выдается за
   уже выполненную source проверку.
   Пять restart crash frontiers уже локально проверены B3. Старые model/parity regressions
   остаются regressions прежнего принятого пути, не operational qualification.

До передачи reviewer обязателен внешний T40 extracted-source receipt с exact
source/archive pins. После него — единый source handoff, без промежуточного ELF.
Не считать scenario map acceptance и не подменять extracted run обычным unit PASS.
Не создавать новый reconnect
design review и не выпускать промежуточный ELF.
Source review пакет — после обеих итераций; промежуточный ZIP не создаётся.
VPS/таймеры/FINAM/credentials/DB0 не затронуты, real orders и live micro закрыты.
Проверки и точный delta: `reports/operations/candle-ws-primary-source-20261103/verification.json`
основного проекта (рядом с рабочим `source/`). Git push/merge не выполнялся.
