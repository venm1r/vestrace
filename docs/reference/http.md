# HTTP: идентичность, ошибки и согласованность

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Граница справочника

Справка описывает выбранный срез, а не обещает доступность всех planned routes. [Каталог](route-catalog.md) отражает регистрации inventory; текущий OpenAPI проекта и proposed MW OpenAPI — разные артефакты. Наличие route descriptor говорит об ожидаемой authorization boundary, не о доказанном успешном execution.

## Идентичность

Bearer-токен разрешается в workspace/principal. Полученные identity values заменяют клиентские headers. Невалидный, истёкший или отозванный token не должен раскрывать, какая именно разновидность отказа помогла угадать credential. Storage failure authentication отличается от invalid credential: в текущем middleware это `503`, а не маскировка под `401`.

Health endpoints `/health/live` и `/health/ready` являются bounded public probes; `/metrics` требует авторизации. Capability и risk определяются inventory и текущей policy. Permission на чтение не эквивалентен разрешению передать data модели или экспортировать его.

## Повтор и конкурентное изменение

Для новых логических записей клиент создаёт свой устойчивый `Idempotency-Key`. Одинаковая повторная HTTP-попытка использует тот же ключ; другая команда — другой. Ключ не является capability. Текущий memory handler сохраняет legacy fallback на `x-request-id`; новые клиенты должны использовать явный idempotency header и не полагаться на него.

`If-Match` в текущем memory revision handler — строка с числом `u32`, не универсальный ETag и не `W/"..."`. Нельзя переносить proposed strong precondition MW на legacy endpoint без явной версии. Run version и memory content revision тоже не одно и то же.

## Ошибки

`ApiError` и конкретный handler — источник точных status/code. Не использовать любую 4xx как сигнал отсутствия объекта и не повторять любой 5xx вслепую. Unknown side effect означает необходимость reconciliation, а не разрешение SDK автоматически повторить команду.

| Наблюдение | Действие клиента |
| --- | --- |
| 401 | Проверить предоставленный credential, не перебирать workspace headers |
| 403 / policy refusal | Исправить законный scope/grant либо отказаться от операции |
| 404 | Обработать отсутствие в разрешённой области; не пробовать скрытые namespaces |
| Revision/idempotency conflict | Перечитать доступное состояние и принять новое решение |
| 501 | Функция не реализована на этом маршруте; UI не показывает успех |
| 503 / unavailable | Сохранить request identity и выяснить, что было committed, прежде чем повторять mutation |

Таблица — руководство по обработке, не новая глобальная унификация всех существующих кодов ошибок.

## Минимальная совместимость

Additive поля не гарантируют совместимость строгих generated clients. Для каждого изменения проверять schema + реальные responses, deprecation policy и error semantics. Proposed MW endpoints включаются только после implementation/acceptance; их схемы не заменяют runtime schema при сборке текущего клиента.

---
**Основание:** [R01: crates/vestrace-http/src/auth.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs), [R02: crates/vestrace-http/src/route_inventory.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/route_inventory.rs), [S10: crates/vestrace-http/src/api/memory.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs), [S15: apps/console/src/sdk/client.ts](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk/client.ts).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
