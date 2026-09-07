# Диагностика без разрушительных обходов

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

| Симптом | Проверить сначала | Чего не делать |
| --- | --- | --- |
| 401 API | Bearer token и разрешение token store | Подставлять другой workspace header |
| Vite UI виден, запросы 401 | Отличие dev proxy от nginx authentication | Компилировать admin token в JS |
| Bootstrap/root error | Mounted store layout, declared identity, writable material root и отсутствие overlap | Создавать пустой volume и объявлять prerequisites выполненными |
| Schema incompatible | Source/build migration set, successful records и checksums | Редактировать applied SQL/checksums вручную |
| Memory сохранена, search пуст | Identity/policy, lifecycle, channels и generation readiness | Считать сохранение доказательством indexed |
| ContextPack без текста | DTO текущего HTTP среза | Читать section_count как готовый prompt |
| Conflict на revise | Текущую доступную revision и конкурирующего writer | Автоматически повторять с latest version без решения пользователя |
| Outbox растёт | Handler, due/backoff/dead-letter и provider dependency | Удалять pending rows ради зелёного health |
| Worker idle | Configured workspace и предметное состояние работы | Считать idle corruption или success всего Run |
| ResultPrepared после restart | Durable marker, binding/publication phase | Повторять provider или вручную ставить job Succeeded |
| 501 на странице | Реализацию конкретного handler | Имитировать успешный ответ во frontend |

Разделять диагностическую гипотезу и установленную причину. При reproduction фиксировать exact source/runtime и safe observations. Установка, на которой что-то не проверялось, не становится непригодной по умолчанию; но её нельзя называть квалифицированной.

Пример отчёта должен содержать ожидаемое поведение, реальный код/сообщение, минимальный сценарий и область возможного влияния. DB credentials, auth headers и пользовательский content исключаются. Для опасного эффекта сохраняется идентичность и неопределённость, а не только stack trace.

---
**Основание:** [R01: crates/vestrace-http/src/auth.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs), [R15: apps/console/vite.config.ts](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/vite.config.ts), [R16: apps/console/nginx.conf.template](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/nginx.conf.template), [S01: docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md), [S08: crates/vestrace-application/src/outbox.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs), [S10: crates/vestrace-http/src/api/memory.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs), [S11: crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
