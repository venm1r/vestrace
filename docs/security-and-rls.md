# Полномочия, раскрытие содержимого и защита данных

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Слои, которые нельзя объединять

Authentication отвечает, кто обращается. Capability/policy — что ему можно сделать сейчас. Workspace scope — к какой области относится объект. Classification/disclosure policy — кому и куда разрешено раскрыть содержимое. Trust/qualification — каким свойствам можно доверять на основании выполненных проверок. Ни одна из этих вещей не заменяет остальные.

В текущем middleware Bearer identity заменяет присланные клиентом workspace/principal. HTTP inventory содержит capability/risk и bounded health probes. Это найденная структура, а не новый security audit всех handlers.

## Чтение памяти

Detail, history, search, context, export и debug должны проверять сопоставимые правила. Запрет только основного GET бесполезен, если snippet, trace или скачанный export cache раскрывает те же bytes. После revoke новое чтение требует текущих прав; исторический receipt не является вечной capability.

Memory labels в текущем vocabulary не следует превращать в порядок Public→Restricted по имени. Channel Sensitivity и произвольные classification labels — разные измерения. Обычный revision writer запрещает смену/clear label без отдельного перехода. UI должен честно показывать это ограничение.

## Запись и источники

Входной документ не может назначать trusted actor, grants, policy или источник системной инструкции. Human assertion, model output и внешний документ остаются происхождением записи с различимыми типами. Source ID не даёт право читать исходник. В частности, экспорт/импорт не переносит полномочия другой установки.

## Секреты и публикация

Не помещать API keys, DB URLs с паролем и bootstrap keys в обычную Memory, prompts, test fixtures, release evidence или файлы данного архива. Credentials принадлежат контролируемым backend/secret stores. Context inspector и сообщения ошибок должны иметь отдельный разрешённый scope.

Контейнерная Console с proxy token доступна тому, кто достигает её loopback-порта. Её нельзя считать многопользовательским login-продуктом или безопасно публиковать в интернет без нового boundary.

## Проверки для новых функций

На каждой новой поверхности нужны wrong-workspace, wrong-principal, revoked grant, stale policy, forbidden history, hidden snippet/count, export-after-revoke и unknown-route cases. Проверять database invariants под runtime role, включая прямые SQL попытки там, где инвариант назначен DB. Frontend disabled button не является security enforcement.

Ни документационная проверка, ни тест на администраторском соединении не подтверждают production-safe статус. Полные gates закреплены в P02/P05/P12 и конкретных MW-пакетах.

---
**Основание:** [R01: crates/vestrace-http/src/auth.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs), [R02: crates/vestrace-http/src/route_inventory.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/route_inventory.rs), [R04: docker-compose.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml), [R16: apps/console/nginx.conf.template](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/nginx.conf.template), [S05: crates/vestrace-application/src/memory/services.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs), [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md).

[Карта документации](README.md) · [Состояние и ограничения](status.md) · [Реестр источников](maintenance/sources.md)
