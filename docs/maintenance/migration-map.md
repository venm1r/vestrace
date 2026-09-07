# Карта документационной переработки

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Карта изменений документации.

## Применение

Baseline — 07e2977. Это overlay с изменёнными вводными руководствами и новой дорожной картой; не весь репозиторий и не замена каталога `docs/` целиком. Патч содержит только документационные paths и root README.

Нормативные тела specs/ADR, historical evidence, P01–P12, root PLAN.md, SQL, Rust, UI, CI и lockfiles не изменяются. Новый MW subtree сохранён с Git tree `1f69acad8676991b2f435501ef3fd8aec742c3b4`. Чтение старых pinned инструкций остаётся возможным через history.

## Переписано и добавлено

| Документ | Действие |
| --- | --- |
| `README.md` | modify |
| `docs/README.md` | modify |
| `docs/architecture.md` | modify |
| `docs/database-schema.md` | modify |
| `docs/design/consolidation.md` | add |
| `docs/design/context-observability.md` | add |
| `docs/design/execution.md` | add |
| `docs/design/integration-boundaries.md` | add |
| `docs/design/learning-health.md` | add |
| `docs/design/materials.md` | add |
| `docs/design/memory-time.md` | add |
| `docs/design/retrieval-context.md` | add |
| `docs/design/temporal-conflicts.md` | add |
| `docs/design/transactions.md` | add |
| `docs/development/README.md` | add |
| `docs/development/agent-workflow.md` | add |
| `docs/development/release.md` | add |
| `docs/development/testing.md` | add |
| `docs/domain-model.md` | modify |
| `docs/evaluation/README.md` | add |
| `docs/getting-started.md` | modify |
| `docs/guides/console.md` | add |
| `docs/guides/memory-api-exercise.md` | add |
| `docs/history/README.md` | add |
| `docs/implementation/README.md` | modify |
| `docs/maintenance/README.md` | add |
| `docs/maintenance/input-reconciliation.md` | add |
| `docs/maintenance/sources.md` | add |
| `docs/operations/backup-restore.md` | add |
| `docs/operations/deployment.md` | add |
| `docs/operations/runbook.md` | add |
| `docs/operations/troubleshooting.md` | add |
| `docs/plans/README.md` | modify |
| `docs/product/memory-workspace.md` | add |
| `docs/product/overview.md` | add |
| `docs/product/scenarios.md` | add |
| `docs/reference/cli.md` | add |
| `docs/reference/configuration.md` | add |
| `docs/reference/glossary.md` | add |
| `docs/reference/http.md` | add |
| `docs/reference/mcp.md` | add |
| `docs/reference/memory.md` | add |
| `docs/reference/retrieval.md` | add |
| `docs/reference/route-catalog.md` | add |
| `docs/roadmap/README.md` | add |
| `docs/roadmap/adoption.md` | add |
| `docs/roadmap/milestones.md` | add |
| `docs/roadmap/next-actions.md` | add |
| `docs/roadmap/p0-foundation.md` | add |
| `docs/roadmap/p1-memory-workspace.md` | add |
| `docs/roadmap/p2-knowledge-quality.md` | add |
| `docs/roadmap/p3-full-platform.md` | add |
| `docs/roadmap/p4-expansion.md` | add |
| `docs/roadmap/program-mapping.md` | add |
| `docs/roadmap/risks-and-decisions.md` | add |
| `docs/security-and-rls.md` | modify |
| `docs/specs/README.md` | modify |
| `docs/status/open-gaps.md` | add |
| `docs/status.md` | add |

## Дальнейший review

Проверить новые claims относительно источников; документы не превращают enum/source symbol в доказанную функцию. Если локальное дерево отличается от baseline, не перезаписывать файлы архивом вслепую. Проверить patch применимость и отклонения, затем отдельным разрешённым действием интегрировать.

Файл [scope-result.json](scope-result.json) содержит фактический состав patch/сохранённые файлы после финальной проверки. [validation-report.md](validation-report.md) описывает пределы выполнения.

---
[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](sources.md)
