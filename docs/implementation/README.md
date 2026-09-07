# Реализация: один пакет требований на одну функцию

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Активные документы

[Memory Workspace](memory-workspace/README.md) интегрирован в исходный commit `07e2977`. Его исходный code baseline — `6f610253`, и этот pin сохранён как история анализа. Новая документация не переписывает старый манифест, не меняет MW-D решения с Proposed на Accepted и не объявляет реализацию MW готовой.

Полные specs, proposed OpenAPI/schema, examples, file plan и 54 acceptance cases находятся только в этом каталоге. Дорожная карта ссылается на них. Перед выполнением читать [MW-00](memory-workspace/plans/00-preflight.md), [integration](memory-workspace/12-integration.md) и текущее разрешение владельца.

## Как новая дорожная карта связана с реализацией

[Приоритеты](../roadmap/README.md) определяют, зачем и в какой зависимости стоит рассматривать работу. Они не являются global file allowlist. [Feature register](../roadmap/feature-register.json) связывает инициативы с существующими пакетами; дальние P2/P4 функции имеют предметные критерии, но требуют собственного spec/scope review до кода.

При появлении нового пакета он получает единственную canonical specification location. Нельзя создавать независимые конкурирующие копии MW requirements в `docs/specs/` и `docs/superpowers/specs/`. Accepted ADR/spec сохраняют более высокий authority; конфликт исправляется в предлагаемом пакете либо принимается явно названное архитектурное изменение.

---
**Основание:** [R18: docs/implementation/memory-workspace/README.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/implementation/memory-workspace/README.md), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
