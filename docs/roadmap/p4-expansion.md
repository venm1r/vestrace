# P4 — расширение после внешней валидации

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемые функциональные приоритеты; приёмочные проверки не выполнялись.

## Решение о группе

Самостоятельные предложения, запускаемые спросом и готовностью эксплуатации, а не шириной списка конкурентов.

Готовность каждого пункта разделена на source observation и реальную проверку. Здесь ни один пункт не получает новый runtime PASS. Dependencies ниже задают полный сценарий приёмки, а не запрет заранее писать spec или read-only тест.

<a id="f401"></a>

## F401. Командная память и развитая federation

**Зачем:** Несколько команд безопасно работают с общими знаниями на понятных условиях.

**В коде/документах:** Нормативная цель и kernels; полный аудит не выполнялся.

**Переиспользовать:** Существующие capabilities/share grants/mounts и Brain–Face–Organ границы.

**Изменения:**

1. Сначала подтвердить спрос и модель tenancy; затем общий scope, identity lifecycle, SSO/admission и review sharing.
2. Не интерпретировать текущий local single-workspace v1 как обещание готовой enterprise federation.

**Условия приёмки:**

- **F401-AC01:** Реальные независимые principals/tenants и отзыв доступа проходят adversarial matrix.
- **F401-AC02:** Стоимость администрирования проверена пилотом; ни глобальный namespace, ни mount не дают implicit trust.

**Зависимости:** [F306](p3-full-platform.md#f306), [F202](p2-knowledge-quality.md#f202).

**Связь с программами:** Отдельный проект после принятия scope; не дополнительный P13.; release relationship: `post-v1-proposed`.

**Не включать:** Незаметное расширение supported topology.

**Основание:** [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md). Наблюдение source не является тестом реализации.

<a id="f402"></a>

## F402. Managed deployment и эксплуатация нескольких установок

**Зачем:** Развёртывание и сопровождение могут стать услугой после проверки спроса.

**В коде/документах:** Новое направление продукта.

**Переиспользовать:** Installer, restore, telemetry, identity и release qualification; не новая semantic model.

**Изменения:**

1. Выбирать после измеренных self-hosted пилотов; описать SLA/SLO, upgrade/cost model и response ownership.
2. Разделить management plane и data-plane полномочия; billing не даёт доступ к памяти.

**Условия приёмки:**

- **F402-AC01:** Пилот измеряет восстановление/обновление без ручной магии автора.
- **F402-AC02:** Сбой control plane не переписывает canonical state; tenant isolation подтверждена на выбранной топологии.

**Зависимости:** [F306](p3-full-platform.md#f306), [F401](p4-expansion.md#f401).

**Связь с программами:** Отдельный проект после принятия scope; не дополнительный P13.; release relationship: `post-v1-proposed`.

**Не включать:** Запуск SaaS до проверенной сохранности данных.

**Основание:** [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md). Наблюдение source не является тестом реализации.

<a id="f403"></a>

## F403. Дополнительные SDK, коннекторы и форматы

**Зачем:** Расширять доступность только под подтверждённые пользовательские задачи.

**В коде/документах:** Предлагаемое расширение.

**Переиспользовать:** Первый SDK, bounded importer, current data disclosure and source identity.

**Изменения:**

1. Выбирать следующую пару язык/коннектор по воспроизводимому запросу пользователя.
2. Для PDF, медиа или cloud source отдельно согласовать extraction/retention/network threat model.

**Условия приёмки:**

- **F403-AC01:** Новый адаптер проходит ту же contract suite и не меняет authority semantics.
- **F403-AC02:** Есть владелец поддержки и версия fixtures; скрытые external calls отсутствуют.

**Зависимости:** [F107](p1-memory-workspace.md#f107), [F203](p2-knowledge-quality.md#f203).

**Связь с программами:** Отдельный проект после принятия scope; не дополнительный P13.; release relationship: `post-v1-proposed`.

**Не включать:** Каталог десятков неподдерживаемых integrations.

**Основание:** [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md). Наблюдение source не является тестом реализации.

<a id="f404"></a>

## F404. Переносимый development harness

**Зачем:** Переиспользовать методику разработки после накопления доказательств её полезности.

**В коде/документах:** Процесс есть; самостоятельный продукт не заявлен.

**Переиспользовать:** Frozen scope, persistent builder, independent review, evidence и mutation checks.

**Изменения:**

1. Собирать escaped defects, стоимость review, воспроизводимость и portability; сначала улучшать harness для Vestrace.
2. Выделять отдельный tool только при независимом спросе и отдельном владельце.

**Условия приёмки:**

- **F404-AC01:** Чужой небольшой repo воспроизводит ограниченный workflow без E:/ или локальных undocumented assumptions.
- **F404-AC02:** Данные показывают пойманные и пропущенные дефекты, а не только количество review.

**Зависимости:** [F306](p3-full-platform.md#f306).

**Связь с программами:** Отдельный проект после принятия scope; не дополнительный P13.; release relationship: `post-v1-proposed`.

**Не включать:** Второй большой продукт до внешнего использования Vestrace.

**Основание:** [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R14](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/scripts/p04-scope.mjs). Наблюдение source не является тестом реализации.

---
[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
