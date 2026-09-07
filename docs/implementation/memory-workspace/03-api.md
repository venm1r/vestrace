# 03. API и клиентский контракт

**Статус:** proposed additive HTTP surface. Все schemas собраны в [contracts.schema.json](contracts/contracts.schema.json); OpenAPI-компаньон описывает новые paths, а не всю существующую API Vestrace.

## 3.1 Совместимость

Существующие `GET /v1/memories/{id}`, `POST /v1/memories`, `POST /v1/memories/{id}/revisions`, `POST /v1/retrieval/search` сохраняют wire shapes. Новый detail не добавляет confidential content в старый metadata endpoint автоматически.

Нынешний числовой If-Match legacy endpoint не заменяется молча RFC entity-tag. Новая correction использует явные `expected_revision_id` и `expected_state_revision` в теле. В ней If-Match не принимается как скрытый альтернативный источник precondition. Проектная будущая миграция legacy условных заголовков — отдельная работа.

В новых mutation routes обязательны UUID `Idempotency-Key`, явный reason для редакторских решений и отсутствие неизвестных body fields. Authentication middleware определяет RequestContext; caller JSON не содержит workspace/principal overrides. Browser Console использует существующий transport/auth flow; токены не оказываются в URL, localStorage или экспорте.

## 3.2 Новые endpoints

| Method/path | Request / response schema | Доступ и смысл |
|---|---|---|
| GET `/v1/memories` | query → `MemoryPage` | MemoryRead; только разрешённые записи, filters/pagination |
| GET `/v1/memories/{memory_id}/detail` | → `MemoryDetail` | MemoryRead + revision classification; content и текущая версия |
| GET `/v1/memories/{memory_id}/revisions` | cursor → `RevisionPage` | История без догадок о связи старых sources |
| GET `/v1/memories/{memory_id}/revisions/{revision_id}` | → `Revision` | Точная историческая revision, повторная read policy |
| POST `/v1/memories/{memory_id}/corrections` | `CorrectionRequest` → `MutationReceipt` | MemoryWrite + право прочитать основания; новая revision, не overwrite |
| POST `/v1/context-packs` | `ContextRequest` → `ContextResponse` | ContextRetrieve; approved destination и реальное содержимое |
| GET `/v1/source-collections` | → `CollectionPage` | MemoryRead; без global scope enumeration |
| POST `/v1/source-collections` | `CreateCollectionRequest` → `Collection` | MemoryWrite, reason, audit, receipt |
| GET `/v1/source-collections/{collection_id}/sources` | cursor → `SourcePage` | Разрешённые source identities и состояние |
| GET `/v1/sources/{source_id}/revisions` | cursor → `SourceRevisionPage` | История sources; payload раскрывается только с content permission |
| POST `/v1/source-imports` | `ImportPreviewRequest` → `ImportPreview` | MemoryWrite; staged inputs не становятся active знанием |
| GET `/v1/source-imports/{operation_id}` | → `ImportOperation` | Автор либо явно разрешённый оператор; реальный progress |
| GET `/v1/source-imports/{operation_id}/items/{item_id}` | → `ImportItemDetail` | Preview diff и три basis только после read checks |
| POST `/v1/source-imports/{operation_id}/apply` | `ApplyImportRequest` → `ImportOperation` | Повторная авторизация каждого item; ответ 202 не означает indexed |
| POST `/v1/source-imports/{operation_id}/cancel` | `CancelRequest` → `ImportOperation` | Только непроведённые items; committed data сохраняются |
| GET `/v1/source-conflicts/{conflict_id}` | → `ConflictDetail` | Видимые основания B/I/M, expected versions |
| POST `/v1/source-conflicts/{conflict_id}/resolve` | `ResolveConflictRequest` → `MutationReceipt` | Атомарное решение + revision/binding |
| POST `/v1/memory-exports` | `ExportRequest` → `ExportOperation` | ExportRead + MemoryRead + разрешённые revisions |
| GET `/v1/memory-exports/{operation_id}` | → `ExportOperation` | Состояние, expiry, безопасные counts |
| GET `/v1/memory-exports/{operation_id}/content` | → package JSON / Markdown | Повторная проверка прав всего pinned набора перед отправкой |

Право на исходный Event не выводится автоматически из права на memory. В первой версии UI может видеть self-contained explanation и provenance_status=partial без запрещённых названий, ID и counts. Неизвестный/недоступный объект возвращает одинаковое 404; операция, явно запрещённая при допустимом доступе к объекту, — 403.

## 3.3 Detail и история

`MemoryDetail` включает `id`, `kind`, `status`, `state_revision`, `active_revision` и `can_correct`. `active_revision` может быть null для объекта без активного содержимого; в этом случае editor выключен. `Revision` содержит identity, ordinal, content, classification, created_at, validity, reason, видимые source refs и `provenance_status`.

`can_correct` — подсказка UI на момент ответа, а не capability. Сервер всегда повторяет проверку при POST. Restore читает указанную старую revision под текущими правами и создаёт новую с reason; classification остаётся действующей, запрещённое старое содержимое не восстанавливается.

## 3.4 ContextPack

Возвращается не массив технических IDs, а `sections[].items[].text` с source/revision refs и representation level. `rendered_context` строится детерминированно из тех же элементов. HTML и instructions не повышают privilege. Content отсутствует или withheld — item не подменяется `explanation` как псевдоисточником.

`max_utf8_bytes` — жёсткий лимит всего rendered_context вместе с citation labels и separators. `estimated_tokens` маркирован как оценка. Не выдавать существующий ceil(bytes/4) [S12] за строгую гарантию.

Optional `token_budget` допускается только вместе с `tokenizer_id`, зарегистрированным и проверенным для выбранного model/destination. Если счётчик недоступен — TOKEN_COUNTER_UNAVAILABLE, не silent fallback. Если token_budget не задан, `token_budget_enforced=false`: это байтово ограниченная выдача, не заявка на квалификацию нормативного hard-token profile. При включении model-token режима требуется точный счётчик текста окончательного rendered_context. Внешнее envelope модели остаётся ответственностью клиента и явно не входит в этот лимит.

API использует существующий RetrievalService, а не вторую систему ranking. Epoch, policy и source eligibility проверяются до выдачи; запрет нельзя обойти переформулированным query, большим budget или history mode. При изменении поколения между поиском и hydration — bounded retry один раз без provider side-effect либо typed conflict, не смесь поколений.

## 3.5 Ошибки

| Code | HTTP | Действие клиента |
|---|---:|---|
| VALIDATION_ERROR | 400 | Исправить данные, не повторять автоматически |
| IDEMPOTENCY_KEY_REQUIRED | 400 | Создать один UUID для логической операции |
| NOT_FOUND | 404 | Не раскрывать, существовал ли объект в другом scope |
| POLICY_DENIED | 403 | Не обходить через другой endpoint |
| REVISION_CONFLICT | 409 | Получить свежий detail, сравнить изменения |
| IDEMPOTENCY_CONFLICT | 409 | Не использовать старый key для нового намерения |
| PREVIEW_STALE | 409 | Построить новый preview, не применять старое согласие |
| COLLECTION_BUSY | 409 | Завершить/отменить текущий apply, не запускать вторую очередь |
| CURSOR_EXPIRED | 409 | Начать browse заново |
| CLASSIFICATION_TRANSITION_REQUIRED | 409 | Редактор/импорт не меняют label |
| INPUT_LIMIT_EXCEEDED | 413 | Уменьшить документ/набор; никаких обрезанных записей |
| TOKEN_COUNTER_UNAVAILABLE | 422 | Выбрать поддерживаемый счётчик или явно bytes-only request |
| PREVIEW_EXPIRED / EXPORT_EXPIRED | 410 | Повторить подготовку; не читать старый cached payload |
| RETRIEVAL_NOT_READY / MATERIAL_UNAVAILABLE | 503 | Показать blocker, не объявлять пустой успешный результат |

Ошибка не содержит сырой SQL, content, credentials, DEK или запрещённые foreign IDs. request_id пригоден для диагностики и не является idempotency key. API Error расширяет текущий ApiErrorBody аддитивно безопасными details, сохраняя code/message.

## 3.6 Клиент

Расширить существующий TypeScript transport файла `apps/console/src/sdk/client.ts`; типизированные memory методы вынести в `memoryClient.ts`. При 409 draft остаётся в оперативной памяти UI. При неизвестном результате POST повторять только с тем же key и тем же body; не делать автоматический overwrite. Выход/смена workspace очищают content caches и draft. Query key всегда включает authenticated scope, resource, revision и фильтры.

MCP получает безопасный memory detail через тот же query service. Он не обходит policy, не возвращает raw material references и не принимает principal от модели. Новый write-tool и внешняя DSH интеграция не входят в этот пакет; улучшение чтения MCP сопровождается regression tests текущих tool names.
