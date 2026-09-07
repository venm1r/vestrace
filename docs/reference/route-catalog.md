# Выбранные маршруты текущего inventory

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

Это **выборка** релевантных деклараций из route inventory, не полный OpenAPI и не доказательство реализации handlers. Существующие `/ag-ui/*` ниже не равны целевому полноценному pinned AG-UI из P08. Для новой функции нужно прочитать handler, composition, authorization и runtime evidence.

| Метод | Путь | Capability/exposure | Risk |
| --- | --- | --- | --- |
| `GET` | `/health/live` | `PublicBounded` | `Low` |
| `GET` | `/health/ready` | `PublicBounded` | `Low` |
| `GET` | `/metrics` | `AuditRead` | `Low` |
| `POST` | `/v1/events` | `EventWrite` | `Medium` |
| `POST` | `/v1/memories` | `MemoryWrite` | `Medium` |
| `GET` | `/v1/memories/{id}` | `MemoryRead` | `Low` |
| `DELETE` | `/v1/memories/{id}` | `MemoryPurge` | `Critical` |
| `POST` | `/v1/memories/{id}/revisions` | `MemoryWrite` | `Medium` |
| `POST` | `/v1/retrieval/search` | `ContextRetrieve` | `Medium` |
| `GET` | `/v1/runs` | `ExecutionRead` | `Low` |
| `POST` | `/v1/runs` | `ExecutionWrite` | `High` |
| `GET` | `/v1/runs/{id}` | `ExecutionRead` | `Low` |
| `POST` | `/v1/runs/{id}/steps` | `ExecutionWrite` | `High` |
| `POST` | `/v1/runs/{id}/pause` | `ExecutionWrite` | `High` |
| `POST` | `/v1/runs/{id}/resume` | `ExecutionWrite` | `High` |
| `POST` | `/v1/runs/{id}/cancel` | `ExecutionWrite` | `High` |
| `POST` | `/v1/runs/{id}/approve` | `ExecutionWrite` | `Critical` |
| `GET` | `/v1/connections` | `WorkspaceAdmin` | `Low` |
| `POST` | `/v1/connections` | `WorkspaceAdmin` | `Critical` |
| `POST` | `/v1/connections/{id}/revisions` | `WorkspaceAdmin` | `Critical` |
| `POST` | `/v1/connections/{id}/admission-policies` | `WorkspaceAdmin` | `Critical` |
| `POST` | `/v1/connections/{id}/qualifications` | `ProviderWrite` | `High` |
| `POST` | `/v1/connections/{id}/credentials` | `WorkspaceAdmin` | `Critical` |
| `POST` | `/v1/models/{id}/revisions` | `ModelWrite` | `Medium` |
| `GET` | `/ag-ui/endpoints` | `ExecutionRead` | `Low` |
| `POST` | `/ag-ui/run` | `ExecutionWrite` | `High` |
| `GET` | `/ag-ui/events/stream` | `ExecutionRead` | `Low` |
| `POST` | `/v1/embedding-jobs/{id}/acknowledge-unknown` | `EmbeddingRetryAfterUnknown` | `Critical` |
| `POST` | `/v1/embedding-transitions/{id}/acknowledge-carry` | `EmbeddingRetryCarriedTransitionBatchAfterUnknown` | `Critical` |

В частности, присутствие admission-policy route в inventory не позволяет повторять старую фразу «маршрута вообще нет». Но без проверки handler не следует делать обратный вывод о его завершённости. Полный runtime schema остаётся в `schemas/` репозитория, а новый MW OpenAPI сохраняет имя `openapi.proposed.json`.

Машиночитаемая версия: [route-catalog.json](route-catalog.json).

---
**Основание:** [R02: crates/vestrace-http/src/route_inventory.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/route_inventory.rs), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
