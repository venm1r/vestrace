# P0 — фундамент и блокеры доверия

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемые функциональные приоритеты; приёмочные проверки не выполнялись.

## Решение о группе

Не расширять публичные обещания, пока не замкнуты используемые production paths и сохранность данных.

Готовность каждого пункта разделена на source observation и реальную проверку. Здесь ни один пункт не получает новый runtime PASS. Dependencies ниже задают полный сценарий приёмки, а не запрет заранее писать spec или read-only тест.

<a id="f001"></a>

## F001. Завершённый embedding/retrieval lifecycle

**Зачем:** Сохранённые данные становятся пригодными для поиска без ручного создания completion records.

**В коде/документах:** Опубликовано частичное принятие; не весь lifecycle.

**Переиспользовать:** P04/14B–14D, общие provider/effect/material authorities, существующий worker. Не реализовывать заново уже принятые блоки.

**Изменения:**

1. Довести delivery binding/publication и остальные предусмотренные P04 rebuild/query paths отдельными принятыми контрактами.
2. Подключить штатную композицию worker; различать ResultPrepared, Live, готовую generation и успешный job.
3. Не открывать context/import readiness через legacy embedding bypass.

**Условия приёмки:**

- **F001-AC01:** Реальный API/worker проходит полный допустимый цикл на runtime-role PostgreSQL.
- **F001-AC02:** После остановки процесса до/после публикации наблюдается один канонический результат и нет повторного внешнего dispatch.
- **F001-AC03:** Все источники и запрос используют точные space/generation; частичные публикации и stale attempts отклоняются.

**Зависимости:** [F002](p0-foundation.md#f002), [F004](p0-foundation.md#f004), [F006](p0-foundation.md#f006).

**Связь с программами:** P04; release relationship: `required-frozen-v1`.

**Не включать:** Новая модель embeddings, новая очередь или присвоение Succeeded администратором.

**Основание:** [S01](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md), [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md). Наблюдение source не является тестом реализации.

<a id="f002"></a>

## F002. Единый доступ к содержимому и полномочия

**Зачем:** Новая библиотека, history, context и export не становятся обходом действующих ограничений.

**В коде/документах:** Есть в исходниках; полного нового замыкания здесь не проверяли.

**Переиспользовать:** Bearer identity, route inventory, capability boundary, workspace RLS и текущие classification/share checks.

**Изменения:**

1. Составить матрицу операций чтения/раскрытия и записи для HTTP/MCP/UI/worker.
2. Для новых read paths проверять текущие полномочия и конкретную ревизию до раскрытия содержимого.
3. Не принимать actor/workspace/grants из тела импорта; секреты и denied content не включать в ошибки, search snippets и logs.

**Условия приёмки:**

- **F002-AC01:** Посторонний workspace и principal не получают bytes, названия источников или существование закрытых объектов через side channels ответа.
- **F002-AC02:** Отозванный доступ блокирует следующее чтение/скачивание даже при прежнем cache/idempotency receipt.
- **F002-AC03:** Unknown route и missing policy дают явный отказ; прямые DB writes проверяются ограниченной runtime role.

**Зависимости:** Нет новых feature-предшественников; source/scope preflight обязателен..

**Связь с программами:** P02, P05, MW-01, MW-06; release relationship: `required-frozen-v1-and-extension`.

**Не включать:** SSO/federation и новая policy engine.

**Основание:** [R01](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs), [R02](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/route_inventory.rs), [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md). Наблюдение source не является тестом реализации.

<a id="f003"></a>

## F003. Атомарная запись памяти и устойчивый повтор

**Зачем:** Потеря HTTP-ответа или сбой outbox не создают вторую память и не оставляют изменение без audit.

**В коде/документах:** Есть в исходниках; полного нового замыкания здесь не проверяли.

**Переиспользовать:** Текущий CAS, save_memory_with_revision, GovernedMutationRepository::commit_in, save_in существующих портов.

**Изменения:**

1. Один UoW фиксирует revision/source/search projection, audit, outbox и receipt принятого запроса.
2. Семантическая идентичность запроса исключает повторно выделенные UUID; результат связан с actor/operation/scope.
3. Все писатели выбранного memory-scope переходят на общую границу; legacy endpoint не оставляется обходом.

**Условия приёмки:**

- **F003-AC01:** Ошибка записи audit/outbox/receipt откатывает каноническое изменение.
- **F003-AC02:** Два одинаковых параллельных запроса сходятся к тем же ID; тот же ключ с иной семантикой конфликтует.
- **F003-AC03:** Stale version не теряет новое изменение; replay не раскрывает запрещённый позднее content.

**Зависимости:** [F002](p0-foundation.md#f002), [F006](p0-foundation.md#f006).

**Связь с программами:** P02, MW-02; release relationship: `required-extension-safety`.

**Не включать:** Переписать общий runtime или удалить уже работающий CAS.

**Основание:** [S05](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs), [S06](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-infrastructure/src/postgres/memory_repository.rs), [S07](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/governed_mutation.rs), [S08](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs), [S09](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/idempotency.rs). Наблюдение source не является тестом реализации.

<a id="f004"></a>

## F004. Воспроизводимая установка и безопасная Console

**Зачем:** Инженер получает диагностируемую установку без догадок о bootstrap key и авторизации.

**В коде/документах:** Есть в исходниках; полного нового замыкания здесь не проверяли.

**Переиспользовать:** Compose provisioning/migrate/init services, mounted secret store, nginx token proxy и startup checks.

**Изменения:**

1. Описать и испытать один поддерживаемый путь bootstrap/role provisioning, без встраивания ключей в образ.
2. Согласовать development Vite и контейнерный путь авторизации; не помещать credential в VITE_* bundle.
3. Различать liveness, DB/schema readiness и готовность конкретной функции; сохранять все необходимые vault volumes.

**Условия приёмки:**

- **F004-AC01:** Чистая изолированная среда с заранее разрешёнными секретами запускается по опубликованной процедуре.
- **F004-AC02:** Пустой secret store, overlap roots и неверные policy settings дают понятный отказ.
- **F004-AC03:** Прокси Console не публикуется наружу как общий admin endpoint; неверные credentials не дают доступ.

**Зависимости:** Нет новых feature-предшественников; source/scope preflight обязателен..

**Связь с программами:** P05, P06; release relationship: `required-frozen-v1`.

**Не включать:** Облачный installer, автогенерация production secrets или login UX как подмена настоящей security boundary.

**Основание:** [R01](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs), [R04](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml), [R05](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/getting-started.md), [R15](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/vite.config.ts), [R16](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/nginx.conf.template). Наблюдение source не является тестом реализации.

<a id="f005"></a>

## F005. Backup, restore и обновление существующей установки

**Зачем:** Пользователь не теряет долговременную память при обновлении, рестарте или восстановлении.

**В коде/документах:** Целевой контракт и отдельные существующие механизмы.

**Переиспользовать:** P05, существующие migration checks, installation permit/fingerprint и material authorities.

**Изменения:**

1. Квалифицировать согласованную процедуру резервирования БД/WAL и обязательных vault dependencies.
2. Проверить upgrade с существующими revisions/receipts и допустимый режим freeze/drain.
3. Разделить backup БД, knowledge export и полное восстановление установки; не обещать downgrade без сценария.

**Условия приёмки:**

- **F005-AC01:** Восстановление выполняется в отдельной среде и сверяет выбранные канонические ID, историю, права и доступность материалов.
- **F005-AC02:** Отсутствующий ключ/несовместимый migration checksum не заменяется пустой новой установкой.
- **F005-AC03:** Проверка рассматривает сбой на границах freeze, copy, activate и restart, а не только exit backup-команды.

**Зависимости:** [F001](p0-foundation.md#f001), [F002](p0-foundation.md#f002), [F004](p0-foundation.md#f004).

**Связь с программами:** P05, P12, MW-07; release relationship: `required-frozen-v1`.

**Не включать:** Независимый restore runtime, подмена восстановленного журнала фиктивной успешной квалификацией.

**Основание:** [R04](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml), [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md). Наблюдение source не является тестом реализации.

<a id="f006"></a>

## F006. Исполнимые проверки и управляемая стоимость CI

**Зачем:** Обратная связь о регрессиях достаточно быстрая и не пропускает отдельные виды тестов.

**В коде/документах:** Есть в исходниках; полного нового замыкания здесь не проверяли.

**Переиспользовать:** Текущий Rust/Node/Compose CI и существующие scope/protocol checks.

**Изменения:**

1. Разделить быстрые, PostgreSQL, composition/fault и release suites; выделить явный doctest шаг.
2. Измерить disk/RAM/build duration до изменения profiles или test target layout.
3. Проверять продукт через штатные entrypoints; setup failure не объявлять поведенческим RED.

**Условия приёмки:**

- **F006-AC01:** Контрольный doctest/compile_fail действительно запускается выбранной командой.
- **F006-AC02:** Runtime-role regression обнаруживает нарушение без admin privileges.
- **F006-AC03:** После изменения CI сохраняется покрытие классов риска; зелёный job не получен отключением тяжёлой проверки.

**Зависимости:** Нет новых feature-предшественников; source/scope preflight обязателен..

**Связь с программами:** P01, P12, MW-07; release relationship: `required-frozen-v1-and-extension`.

**Не включать:** Порог строк кода как основание дробления crates и урезание тестов ради зелёного бейджа.

**Основание:** [R12](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/.github/workflows/ci.yml), [S16](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/package.json). Наблюдение source не является тестом реализации.

<a id="f007"></a>

## F007. Правдивые статусы и диагностика работы

**Зачем:** Оператор отличает очереди, блокеры и неизвестный исход от успешного исполнения.

**В коде/документах:** Есть в исходниках; полного нового замыкания здесь не проверяли.

**Переиспользовать:** worker --once, outbox attempts/dead-letter, canonical Run projections, health и audit.

**Изменения:**

1. Показать ожидаемый dependency, безопасную причину остановки и разрешённый следующий шаг.
2. Разделить accepted/applied/indexed и process exit/Run result.
3. Метрики не содержат content, токенов или необоснованных счётчиков закрытых объектов.

**Условия приёмки:**

- **F007-AC01:** 0/3/1 bounded worker outcomes трактуются как cycle result, не общая квалификация.
- **F007-AC02:** Unhandled topic остаётся pending и виден оператору; restart не удаляет причину отказа.
- **F007-AC03:** UI после reconnect получает сохранённое состояние, а не оптимистический локальный success.

**Зависимости:** [F002](p0-foundation.md#f002).

**Связь с программами:** P05, P11, MW-04; release relationship: `required-frozen-v1-and-extension`.

**Не включать:** Автоматический retry любого UNKNOWN.

**Основание:** [S01](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md), [S08](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs), [R03](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-cli/src/main.rs). Наблюдение source не является тестом реализации.

<a id="f008"></a>

## F008. Содержимое, retention и удаление без обходов

**Зачем:** Импорт и экспорт не создают неуправляемые копии чувствительного содержимого.

**В коде/документах:** Есть в исходниках; полного нового замыкания здесь не проверяли.

**Переиспользовать:** MaterialIntentCommands, vault, erasure/blockers, существующие export/purge policy contracts.

**Изменения:**

1. Разрешить source staging/download только через законного owner/read пути материалов.
2. Определить сроки хранения preview, export, context evidence и отношение к удержанию источников.
3. Удаление по политике не подменять перемещением записи в корзину или сохранением plaintext в audit.

**Условия приёмки:**

- **F008-AC01:** Prepared материал не читается как Live; утраченное право прекращает новые downloads.
- **F008-AC02:** Отмена/ошибка импорта не оставляет доступный бесхозный staging.
- **F008-AC03:** Область erasure-проверки включает действующие managed copies и явно не обещает отзыв внешнего экспорта.

**Зависимости:** [F002](p0-foundation.md#f002), [F005](p0-foundation.md#f005).

**Связь с программами:** P02, P05, P07, MW-04, MW-06; release relationship: `required-frozen-v1-and-extension`.

**Не включать:** Новый vault, plaintext временные файлы как fallback и стирание исходных audit facts.

**Основание:** [S17](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/material/commands.rs), [R04](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml), [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md). Наблюдение source не является тестом реализации.

---
[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
