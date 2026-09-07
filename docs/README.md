# Документация Vestrace

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Дорожная карта и приоритеты

[Дорожная карта развития](roadmap/README.md) разделяет 34 инициативы на P0–P4 и связывает их с существующими P01–P12/MW-00–MW-07. [Milestones](roadmap/milestones.md) и [ближайшие задачи](roadmap/next-actions.md) задают проверяемые результаты вместо обещанных дат. [Корпус качества](evaluation/README.md) и [сверка входных документов](maintenance/input-reconciliation.md) дополняют план.

## Выберите маршрут чтения

**Пользователь или интегратор:** [обзор](product/overview.md) → [состояние](status.md) → [подготовка среды](getting-started.md) → [текущий Memory API](guides/memory-api-exercise.md) → [HTTP](reference/http.md) или [MCP](reference/mcp.md).

**Разработчик:** [архитектура](architecture.md) → [домен](domain-model.md) → [границы записи](design/transactions.md) → [разработка](development/README.md) → [тестирование](development/testing.md) → [планирование](plans/README.md).

**Оператор:** [развёртывание](operations/deployment.md) → [безопасность](security-and-rls.md) → [повседневные операции](operations/runbook.md) → [обновление и восстановление](operations/backup-restore.md) → [диагностика](operations/troubleshooting.md).

**Исполнитель Memory Workspace:** [одна страница о направлении](product/memory-workspace.md) → [пакет реализации](implementation/memory-workspace/README.md) → [MW-00 preflight](implementation/memory-workspace/plans/00-preflight.md). Не начинать с отдельной схемы или случайной задачи без требований пакета.

## Детализация следующих функций

[Temporal/conflicts](design/temporal-conflicts.md) · [консолидация](design/consolidation.md) · [инспектор контекста](design/context-observability.md) · [внешний агент](design/integration-boundaries.md). Эти документы — proposals после соответствующих priority/dependency gates.

## Полная карта

| Раздел | Документы |
| --- | --- |
| Продукт | [Обзор](product/overview.md), [сценарии](product/scenarios.md), [Memory Workspace](product/memory-workspace.md) |
| Текущее состояние | [Реестр возможностей](status.md), [пробелы](status/open-gaps.md) |
| Начало работы | [Подготовка](getting-started.md), [API-упражнение](guides/memory-api-exercise.md), [Console](guides/console.md) |
| Архитектура | [Карта](architecture.md), [домен](domain-model.md), [память и время](design/memory-time.md), [retrieval](design/retrieval-context.md), [исполнение](design/execution.md), [транзакции](design/transactions.md), [материалы](design/materials.md), [обучение и health](design/learning-health.md) |
| Справка | [HTTP](reference/http.md), [маршруты](reference/route-catalog.md), [Memory](reference/memory.md), [retrieval](reference/retrieval.md), [MCP](reference/mcp.md), [CLI](reference/cli.md), [конфигурация](reference/configuration.md), [словарь](reference/glossary.md) |
| Эксплуатация | [Развёртывание](operations/deployment.md), [runbook](operations/runbook.md), [обновление/restore](operations/backup-restore.md), [ошибки](operations/troubleshooting.md), [БД](database-schema.md), [безопасность](security-and-rls.md) |
| Разработка | [Старт](development/README.md), [тесты](development/testing.md), [workflow агентов](development/agent-workflow.md), [приёмка релиза](development/release.md) |
| Решения и планы | [Нормативный индекс](specs/README.md), [карта программ](plans/README.md), [расширения реализации](implementation/README.md) |
| Сопровождение документов | [Правила](maintenance/README.md), [источники](maintenance/sources.md), [карта перехода](maintenance/migration-map.md), [отчёт проверки](maintenance/validation-report.md) |
| История | [Архив и сохранённые основания](history/README.md) |

## Как читать статусы

«Найдено в исходниках» — статическое наблюдение, а не успешный запуск. «Зафиксировано в evidence» — результат указанного исполнителя на указанном срезе, а не новый независимый прогон. «Нормативный контракт» — требование. «Предлагаемый проект» — решение для review. «Не проверено» — ограничение знания, а не автоматически отсутствие функции.

Текущий реестр — [status.md](status.md). Старый файл `current-implementation.md` остаётся историческим документом своего baseline и не является обновляемой сводкой этой редакции.

---
[Карта документации](README.md) · [Состояние и ограничения](status.md) · [Реестр источников](maintenance/sources.md)
