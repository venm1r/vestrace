# P2 — качество знания и интеграция

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемые функциональные приоритеты; приёмочные проверки не выполнялись.

## Решение о группе

Повышать полезность и объяснимость на измеряемых задачах; не добавлять автономность без основания.

Готовность каждого пункта разделена на source observation и реальную проверку. Здесь ни один пункт не получает новый runtime PASS. Dependencies ниже задают полный сценарий приёмки, а не запрет заранее писать spec или read-only тест.

<a id="f201"></a>

## F201. Временное знание: current, as_of и история осведомлённости

**Зачем:** Система различает старое решение, позднее исправление и сведения, доступные в прошлом.

**В коде/документах:** Есть temporal/revision механизмы, полный смысловой контракт не доказан.

**Переиспользовать:** TimePerspective, revision hydrator, occurred/recorded/validity контракты; не вводить вторую timeline.

**Изменения:**

1. Зафиксировать отдельные semantics validity time и recorded-as-known; не объявлять их одним as_of.
2. Сохранять exact revision refs в ответе и не подменять их latest.
3. Закрыть недостатки хранения/индексации отдельным контрактом до обещания полного bitemporal replay.

**Условия приёмки:**

- **F201-AC01:** Один корпус с поздним исправлением даёт разные ожидаемые ответы для текущего и исторических вопросов.
- **F201-AC02:** Superseded не выдаётся как current, а ретроспективный read всё ещё требует текущего доступа.
- **F201-AC03:** Вопрос без достаточной истории даёт явную границу знания, не реконструированный вымысел.

**Зависимости:** [F101](p1-memory-workspace.md#f101), [F102](p1-memory-workspace.md#f102).

**Связь с программами:** Отдельный проект после принятия scope; не дополнительный P13.; release relationship: `existing-normative-depth-needs-explicit-placement`.

**Не включать:** Дата документа как доверенное время события и фиктивное знание прошлого.

**Основание:** [S11](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs), [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R13](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs). Наблюдение source не является тестом реализации.

<a id="f202"></a>

## F202. Provenance и управляемые смысловые конфликты

**Зачем:** Ответ можно проверить по источникам, а противоречие разрешить без скрытого last-write-wins.

**В коде/документах:** Доменный контракт и частичные repository/application пути.

**Переиспользовать:** Claim/Conflict/Reconciliation domain, cognitive mutation repository, MW correction UI.

**Изменения:**

1. Различать sync conflict bytes и смысловой конфликт утверждений.
2. Показывать permitted support/contradiction refs и принятое решение; revision-lineage не достраивать догадкой.
3. Ввести impact view для изменений, не переписывая прошлый Run context.

**Условия приёмки:**

- **F202-AC01:** Два допустимых противоречащих источника остаются явными до lawful resolution.
- **F202-AC02:** Источники с разными правами не дают закрытый факт через публичную сводку.
- **F202-AC03:** Отклонённое или новое разрешение не стирает прошлые основания и actor identity.

**Зависимости:** [F103](p1-memory-workspace.md#f103), [F104](p1-memory-workspace.md#f104), [F201](p2-knowledge-quality.md#f201).

**Связь с программами:** Отдельный проект после принятия scope; не дополнительный P13.; release relationship: `existing-normative-depth-needs-explicit-placement`.

**Не включать:** Процент confidence как вероятность истинности и автоматическое доверие из частоты использования.

**Основание:** [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [S06](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-infrastructure/src/postgres/memory_repository.rs). Наблюдение source не является тестом реализации.

<a id="f203"></a>

## F203. Одна квалифицированная внешняя memory-интеграция

**Зачем:** DSH или другой выбранный runtime использует знания Vestrace между сессиями.

**В коде/документах:** Есть MCP-граница с ограниченной выдачей.

**Переиспользовать:** MCP tools, content read gate, один typed client, existing auth binding.

**Изменения:**

1. Первый вариант read-only; затем explicit remember, затем отдельно opt-in capture.
2. Задать optional-memory и required-context policy приложением; capture создаёт наблюдения с источниками.
3. Зафиксировать версии клиентов и один межсессионный сценарий; runtime не получает контроль над остальными effects через факт подключения памяти.

**Условия приёмки:**

- **F203-AC01:** Новая сессия получает принятую коррекцию; чужая сессия/identity не получает запись.
- **F203-AC02:** Недоступность памяти даёт документированный fail mode, а не скрытую смену prompt.
- **F203-AC03:** Capture повтор не дублирует знание; model output не становится автоматически подтверждённым фактом.

**Зависимости:** [F108](p1-memory-workspace.md#f108), [F102](p1-memory-workspace.md#f102), [F002](p0-foundation.md#f002).

**Связь с программами:** Отдельный проект после принятия scope; не дополнительный P13.; release relationship: `proposed-extension`.

**Не включать:** Маркетинговый статус партнёрства, all-tool governance чужого runtime и все интеграции сразу.

**Основание:** [R17](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-mcp/src/server.rs), [R01](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs). Наблюдение source не является тестом реализации.

<a id="f204"></a>

## F204. Измеряемое качество памяти и регрессии

**Зачем:** Каждое усложнение памяти оправдано результатом на задачах, а не числом новых тестов.

**В коде/документах:** Проверочные механизмы есть; новый корпус предлагается.

**Переиспользовать:** Existing eval facts отделены от learned projections; MW acceptance и синтетические источники.

**Изменения:**

1. Собрать versioned corpus: актуальность, противоречия, отсутствие сведений, права, удаление, несколько сессий.
2. Сравнить с простым FTS baseline на одинаковых данных/model/token budget.
3. Отдельно измерять retrieval evidence и качество ответа модели; ACL gates не отдавать model judge.

**Условия приёмки:**

- **F204-AC01:** Для каждого case есть ожидаемые разрешённые source IDs и запрещённые выводы.
- **F204-AC02:** Изменение tokenizer/model/retrieval settings создаёт новый experiment, а не переписывает старый результат.
- **F204-AC03:** Регрессия на hard safety case блокирует публикацию даже при улучшении средней оценки.

**Зависимости:** [F101](p1-memory-workspace.md#f101), [F102](p1-memory-workspace.md#f102).

**Связь с программами:** MW-07; release relationship: `proposed-extension`.

**Не включать:** Универсальный процент качества, доказательство рынка по stars или model judge как security oracle.

**Основание:** [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [S11](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs). Наблюдение source не является тестом реализации.

<a id="f205"></a>

## F205. Инспектор контекста и объяснение решений

**Зачем:** Инженер понимает, почему ответ опирался на конкретные сведения и почему новая версия изменила результат.

**В коде/документах:** Есть журнал/контекст-компоненты; новый инспектор предлагается.

**Переиспользовать:** Retrieval journals, exact revision refs, ModelRequestEvidence, audit/read gates.

**Изменения:**

1. Показать разрешённую цепочку query/filter/candidates/sections/rendered payload.
2. Разделить issued context, client acknowledgement и independently observed model request.
3. Ограничить trace retention и доступ; контекст не становится вечной скрытой копией удалённого знания.

**Условия приёмки:**

- **F205-AC01:** Сравнение двух конфигураций показывает изменившиеся source revisions и причины.
- **F205-AC02:** Пользователь без доступа не получает candidates через debug endpoint.
- **F205-AC03:** Отсутствующее request observation честно помечено; сохранённый payload не считается доказательством использования моделью.

**Зависимости:** [F102](p1-memory-workspace.md#f102), [F002](p0-foundation.md#f002), [F008](p0-foundation.md#f008).

**Связь с программами:** Отдельный проект после принятия scope; не дополнительный P13.; release relationship: `proposed-extension`.

**Не включать:** Обещание полного deterministic LLM output replay.

**Основание:** [S11](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs), [S12](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/retrieval/context_builder.rs), [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md). Наблюдение source не является тестом реализации.

<a id="f206"></a>

## F206. Фоновая консолидация и обновляемые справки

**Зачем:** Перед новой задачей можно получить короткую актуальную справку с проверяемыми основаниями.

**В коде/документах:** Новое продуктовое предложение.

**Переиспользовать:** Memory revisions, derivation, existing execution/outbox, learned projections; no independent reflect runtime.

**Изменения:**

1. Один тип справки: решения, ограничения, незавершённые вопросы проекта.
2. Записывать source revision set, метод/model revision, validity watermark и stale состояние.
3. Обновление источника инвалидирует справку; сохранение нового канонического вывода требует отдельной mutation.

**Условия приёмки:**

- **F206-AC01:** Справка указывает на exact inputs и не наследует более широкие права, чем источники.
- **F206-AC02:** Исправление/отзыв источника помечает stale до нового использования.
- **F206-AC03:** Ошибка фоновой задачи не уничтожает предыдущие canonical facts; повтор сходится по job/input identity.

**Зависимости:** [F201](p2-knowledge-quality.md#f201), [F202](p2-knowledge-quality.md#f202), [F204](p2-knowledge-quality.md#f204), [F008](p0-foundation.md#f008).

**Связь с программами:** Отдельный проект после принятия scope; не дополнительный P13.; release relationship: `proposed-extension`.

**Не включать:** Самоизменение policies, автоистина и массовые бессрочные background agents.

**Основание:** [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [S08](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs). Наблюдение source не является тестом реализации.

<a id="f207"></a>

## F207. Стоимость retrieval и инкрементальных представлений

**Зачем:** Изменение одного источника не требует бесполезного пересоздания всего корпуса.

**В коде/документах:** Частичные проекции и механизм generations.

**Переиспользовать:** P04 spaces/generations, source revisions, explicit processing recipes.

**Изменения:**

1. Разделить новые bytes, новую parser recipe и новый embedding model как разные причины переработки.
2. Зафиксировать warm/cold latency, memory/disk и объём пересчитанных элементов.
3. Оптимизировать только после correctness baseline; не ослаблять guards ради benchmark.

**Условия приёмки:**

- **F207-AC01:** Неизменившиеся данные не создают новых canonical revisions или случайного нового provenance.
- **F207-AC02:** Ready generation не содержит смесь несогласованных spaces.
- **F207-AC03:** Измерение сравнивает одинаковое оборудование/данные и сообщает trade-offs, а не только лучший run.

**Зависимости:** [F001](p0-foundation.md#f001), [F105](p1-memory-workspace.md#f105).

**Связь с программами:** P04; release relationship: `proposed-optimization`.

**Не включать:** Собственная vector DB и преждевременный distributed deployment.

**Основание:** [S01](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md), [S11](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs). Наблюдение source не является тестом реализации.

---
[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
