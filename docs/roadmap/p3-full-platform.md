# P3 — полнота действующей v1.0 платформы

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемые функциональные приоритеты; приёмочные проверки не выполнялись.

## Решение о группе

Продуктово последующая глубина; все перечисленные frozen-v1 обязательства остаются обязательными для полного v1.0.

P3 не означает «необязательно для v1.0». Полный frozen v1 остаётся зависимым от P06–P12; эта группа может выполняться параллельной независимой веткой после P05, не ожидая всех P2 proposals.

Готовность каждого пункта разделена на source observation и реальную проверку. Здесь ни один пункт не получает новый runtime PASS. Dependencies ниже задают полный сценарий приёмки, а не запрет заранее писать spec или read-only тест.

<a id="f301"></a>

## F301. Полноценные providers, models и agent Runs

**Зачем:** Агентный сценарий запускается по реальной конфигурации и сохраняет выбранную идентичность провайдера.

**В коде/документах:** Частично реализованный governed execution foundation.

**Переиспользовать:** P03 model/connection revisions, q1 qualification, ModelBindingSnapshot, shared provider graph.

**Изменения:**

1. Довести P06 по существующему frozen scope, включая no_auth/Bearer ветки и реальные модели.
2. Не делать выбор provider через текущую environment вместо принятого binding.

**Условия приёмки:**

- **F301-AC01:** Браузер/HTTP создают допустимую конфигурацию; после рестарта Run использует pinned revisions.
- **F301-AC02:** Отказ qualification/credentials предотвращает dispatch и имеет safe diagnostics.

**Зависимости:** [F005](p0-foundation.md#f005), [F004](p0-foundation.md#f004), [F002](p0-foundation.md#f002).

**Связь с программами:** P06; release relationship: `required-frozen-v1`.

**Не включать:** Десятки провайдеров до закрытия одного полного пути.

**Основание:** [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R05](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/getting-started.md). Наблюдение source не является тестом реализации.

<a id="f302"></a>

## F302. Interaction kernel, artifacts и retention

**Зачем:** Сообщения, действия и материалы образуют непротиворечивую историю взаимодействия.

**В коде/документах:** Целевой контракт с отдельными существующими поверхностями.

**Переиспользовать:** P07 canonical Run + interaction/artifact contracts, content materials, retention authority.

**Изменения:**

1. Довести thread/message/state/context kernel и artifact lifecycle через один Run.
2. Согласовать replay low-water и retention без альтернативной session truth.

**Условия приёмки:**

- **F302-AC01:** Restart/reconnect не теряет каноническое действие и не создаёт дубль.
- **F302-AC02:** Удаление или expiry материала отражается в последующих reads/replay согласно контракту.

**Зависимости:** [F301](p3-full-platform.md#f301), [F008](p0-foundation.md#f008).

**Связь с программами:** P07; release relationship: `required-frozen-v1`.

**Не включать:** Новый чат runtime для обхода Run.

**Основание:** [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md). Наблюдение source не является тестом реализации.

<a id="f303"></a>

## F303. Полный согласованный AG-UI

**Зачем:** Официальный клиент получает согласованный поток событий и восстановление.

**В коде/документах:** Есть legacy surfaces; full pinned interop не подтверждён.

**Переиспользовать:** P08, pinned protocol lock, interaction kernel и существующая Console.

**Изменения:**

1. Один предусмотренный endpoint и согласованный client; не развивать legacy split routes как второй протокол.
2. Проверить streaming, replay, interrupt, tools и требуемые мультимодальные ветки frozen scope.

**Условия приёмки:**

- **F303-AC01:** Официальный pinned client проходит interop и reconnect cases.
- **F303-AC02:** Серверные события являются проекцией canonical state; нет UI-only success.

**Зависимости:** [F302](p3-full-platform.md#f302).

**Связь с программами:** P08; release relationship: `required-frozen-v1`.

**Не включать:** Дополнительные версии протокола без отдельного compatibility gate.

**Основание:** [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R02](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/route_inventory.rs). Наблюдение source не является тестом реализации.

<a id="f304"></a>

## F304. A2A server/client и workflow step

**Зачем:** Межагентный обмен работает без параллельной истины об исполнении.

**В коде/документах:** Обязательная цель frozen plan; здесь не квалифицирована.

**Переиспользовать:** P09/P10, canonical Run/effects, connection qualification, pinned SDK/spec.

**Изменения:**

1. Сначала server/card/six core operations по P09, затем outbound workflow step по P10.
2. Проверять SSRF/auth, deadlines, cancellation, retained replay и неизвестный внешний исход.

**Условия приёмки:**

- **F304-AC01:** Официальный contract/TCK и независимый sample server дают записанные interop results.
- **F304-AC02:** Сбой outbound запроса не превращается в permission to retry; удалённое утверждение не повышает local trust.

**Зависимости:** [F302](p3-full-platform.md#f302).

**Связь с программами:** P09, P10; release relationship: `required-frozen-v1`.

**Не включать:** Полная federation или доверие к self-signed card без политики.

**Основание:** [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md). Наблюдение source не является тестом реализации.

<a id="f305"></a>

## F305. Остальные пользовательские workflows Console

**Зачем:** В утверждённом меню нет доступных заглушек и настроек без эффекта.

**В коде/документах:** Есть страницы/endpoints с неполной приёмкой.

**Переиспользовать:** P11 pages/settings/workflows/triggers/evaluations/audit и текущий SDK.

**Изменения:**

1. Закрыть eleven-menu workflows по frozen program; различать read views и применяемые настройки.
2. Повторно использовать новые memory read/write API, не дублировать их в отдельной странице.

**Условия приёмки:**

- **F305-AC01:** Браузерная сценарная проверка и restart охватывают каждую включённую функцию.
- **F305-AC02:** 501/disabled остаются честными до реализации; после включения настройка меняет наблюдаемое runtime поведение.

**Зависимости:** [F303](p3-full-platform.md#f303), [F304](p3-full-platform.md#f304).

**Связь с программами:** P11; release relationship: `required-frozen-v1`.

**Не включать:** No-code editor как новый продукт.

**Основание:** [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [S14](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/main.tsx), [S15](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk/client.ts). Наблюдение source не является тестом реализации.

<a id="f306"></a>

## F306. Квалификация полного v1.0 и совместимость

**Зачем:** Релиз означает конкретное проверенное обещание, а не готовность списка файлов.

**В коде/документах:** Существующий обязательный gate, здесь не выполнялся.

**Переиспользовать:** P12, accepted specs, qualification evidence and source/image/config/model/protocol identities.

**Изменения:**

1. Утвердить точный shipping manifest, supported environment и contract freeze.
2. Включённые MW функции квалифицировать отдельно и добавить к release matrix явным решением.
3. Нельзя заменить full frozen v1.0 более узким alpha под тем же именем.

**Условия приёмки:**

- **F306-AC01:** Все требуемые P12 источники evidence свежие и относятся к тем же target identities.
- **F306-AC02:** Пропущенные/blocked проверки не становятся PASS; release notes перечисляют ограничения и migration policy.

**Зависимости:** [F305](p3-full-platform.md#f305), [F005](p0-foundation.md#f005), [F006](p0-foundation.md#f006).

**Связь с программами:** P12; release relationship: `required-frozen-v1`.

**Не включать:** v1.0 как срок календаря или количество LOC.

**Основание:** [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md). Наблюдение source не является тестом реализации.

---
[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
