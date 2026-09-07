# Текущее состояние и границы утверждений

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Срез

Документация привязана к `07e2977a20b05c5b16953a206a6d68bdbff3a052` (`docs: integrate memory workspace design`). Сравнение с `6f610253` показало одну интеграционную правку документации: runtime-код не изменён. Именно указанный пользователем commit, а не движущийся HEAD, является основанием этой редакции. Незапушенные изменения не входят в срез. Использованы выбранные исходники и сохранённый манифест предшествующего review; полного повторного аудита всех файлов нет.

Здесь **нет новых runtime evidence**. Даже запись «опубликовано в evidence» означает чтение результата автора репозитория, а не повторение его эксперимента в этой среде.

## Реестр

| Возможность | Статус | Граница | Основание |
| --- | --- | --- | --- |
| HTTP identity | **Найдено в коде** | Bearer-токен разрешается сервером; identity-заголовки заменяются. | [R01](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs) |
| Инвентарь маршрутов | **Найдено в коде** | Route descriptor задаёт capability/risk; наличие descriptor не доказывает полноценный handler. | [R02](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/route_inventory.rs) |
| Создание/ревизия Memory | **Найдено в коде** | memory/revision/source/search пишутся вместе; outbox/idempotency затем отдельно. | [S05](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs) |
| Получение памяти HTTP/MCP | **Ограничено** | GET memory отдаёт метаданные; содержимое и номер ревизии не входят в текущий MemoryResponse. | [S10](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs) |
| Библиотека/история для внешнего клиента | **Предложено** | MW проектирует list/detail/history; наличие внутренних ревизий не делает эти routes доступными. | [S03](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/mod.rs) |
| ContextPack HTTP | **Ограничено** | Ответ содержит сводку, но не sections/rendered context. | [S11](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs) |
| Подсчёт токенов | **Ограничено** | Текущий helper использует ceil(UTF-8 bytes/4); это не точный счётчик произвольной модели. | [S12](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/retrieval/context_builder.rs) |
| Точные исторические ссылки | **Найдено в коде** | Hydrator запрашивает указанную ревизию без подмены текущей. | [R13](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs) |
| Один цикл worker | **Опубликовано в evidence** | Exit 0/3/1 различает обработку, idle и ошибку; не является успехом всего Run. | [S01](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md) |
| P04 delivery ResultPrepared | **Опубликовано в evidence** | 14D принят до ResultPrepared; это не Live и не Succeeded. | [S01](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md) |
| Завершение P04 | **Не подтверждено** | 14E и последующие границы не закрыты опубликованным решением 14D. | [S01](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md) |
| Редактор памяти Console | **Предложено** | Есть компонент MemoryConsole, но нет маршрута полноценного memory workspace. | [S14](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/main.tsx) |
| Импорт/синхронизация | **Предложено** | Контракты MW присутствуют только в документационном пакете. | [S03](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/mod.rs) |
| Переносимый JSON/Markdown | **Предложено** | Спецификация MW не является работающим exporter. | [S03](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/mod.rs) |
| Ключи и content materials | **Компоненты найдены** | Наличие lifecycle primitives не квалифицирует все пути чтения, импорта и удаления. | [S17](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/material/commands.rs) |
| Готовность v1.0 | **Не подтверждено** | Пакеты/тесты/документы не заменяют P12 и целевую квалификацию. | [R11](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md) |

Машиночитаемая копия: [capabilities.json](status/capabilities.json). Markdown-таблица генерируется из тех же записей при сборке этой редакции; при дальнейших изменениях оба представления нужно обновлять совместно.

## Как трактовать пробелы

«Не подтверждено» нельзя заменять на «отсутствует во всём репозитории». «Найдено в коде» нельзя заменять на «проверено в production». Ошибка документации и дефект реализации — разные вещи: в руководстве исправляется первое, второе получает задачу и проверку, а не исправляется только формулировкой.

Подробные ограничения и очередность работы — [open-gaps](status/open-gaps.md). Старый pinned implementation snapshot и исторические gap-delta сохранены как основания, но не переписываются задним числом под этот реестр.

---
[Карта документации](README.md) · [Состояние и ограничения](status.md) · [Реестр источников](maintenance/sources.md)
