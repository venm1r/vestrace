# Разработчику: правила изменения проекта

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Начать с контракта

Прочитать [архитектуру](../architecture.md), [реестр состояния](../status.md), [приоритеты](../roadmap/README.md) и точный пакет своей задачи. Нельзя считать ссылку на класс доказательством работающего сценария. При изменении нового source baseline перечитать затронутые paths и dependency evidence.

## Четыре результата задачи

Нужны изменённый контракт, реализация, проверяемое поведение и актуальная документация. Тест проверяет наблюдаемое обязательство пользователя, а не только то, что вызвана внутренняя функция. База/fixtures готовят законное окружение; E2E данные появляются через реальные entrypoints, а не через подмену completion state.

## Границы репозитория

Не менять защищённые frozen specs, historical evidence и applied migrations как побочный эффект новой функции. Root PLAN.md принадлежит отдельной работе. Перед записью зафиксировать exact file allowlist, существующие dirty/untracked bytes и порядок независимого review. Документы MW не дают blanket-разрешение менять P04-scope.

## Очерёдность

По умолчанию один крупный кодовый пакет и одна независимая документационная/измерительная задача. Не планировать одновременно несколько конфликтующих migration/writer changes одним persistent builder. Перед параллельной работой нужны непересекающиеся scope и принятые интерфейсы.

## Модульность

Сохранять domain/application/infrastructure/HTTP/CLI/MCP слои. UI и scanner — клиенты, не новые authority. Выделять интерфейс по ответственности и читабельности, а не произвольному LOC threshold. Не создавать второй event/runtime/policy engine ради удобства новой задачи.

## Конец задачи

Review контракта и runtime composition важнее убедительного рассказа builder. Перечислить выполненные команды, фактические exit codes, нужные негативные проверки и невыполненные условия. Commit/merge/deploy выполняются только по соответствующему разрешению; эта документация их не выполняет.

---
**Основание:** [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R14: scripts/p04-scope.mjs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/scripts/p04-scope.mjs).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
