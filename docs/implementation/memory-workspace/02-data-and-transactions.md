# 02. Модель данных, полномочия и транзакционные границы

**Все новые типы/таблицы ниже — proposed.** Они дополняют existing Memory/MemoryRevision/Event/MemorySource и не заменяют их.

## 2.1 Канонические объекты

| Объект | Назначение и неизменяемая часть | Изменяемая часть |
|---|---|---|
| `SourceCollection` | workspace + локальная identity коллекции | имя и configuration revision; один активный apply на коллекцию |
| `KnowledgeSource` | workspace + collection_id + external_id | текущий source_revision_id, locator, state/version |
| `SourceRevision` | source_id + ordinal + точный payload material ref + формат + заявленная classification + capture event | отсутствует; исправление — следующая версия |
| `MemorySourceBinding` | связь memory с source для детерминированного whole-document представления | source revision, import memory revision, manual_override и version |
| `MemoryRevisionSourceLink` | точная связь memory_revision_id с event/source_revision_id и origin kind | отсутствует |
| `ImportOperation` | автор, workspace, набор входов, режим, выбранная политика и request identity | staging/preview/apply progress; не самостоятельный scheduler |
| `ImportItem` | input identity, saved payload, preview disposition и base versions | применённая receipt либо conflict/cancel outcome |
| `SourceConflict` | тройка B/I/M и версии, вызвавшие конфликт | только отдельное решение с ожидаемой версией |
| `ExportOperation` | автор, pinned selection, режим экспорта, формат | состояние подготовки и ссылка на защищённый результат |

`Memory` остаётся владельцем effective содержимого. SourceRevision никогда не становится альтернативным current memory head. Сырой источник и исправленная пользователем память могут различаться; это допустимое, явно показанное состояние.

## 2.2 Identity и provenance

Идентичность source — `(workspace_id, collection_id, external_id)`, не путь и не content hash. `external_id` — стабильный UUID, выбранный локальным scanner или пользователем. Путь — locator для отображения. Один source первой версии порождает одну MemoryKind::Observation с `origin_kind=source_excerpt`. Это запись содержимого источника, а не автоматически подтверждённый semantic fact.

Импортёр требует явные confidence/importance в настройках preview; они сохраняются как пользовательская оценка. Он не выставляет 1.0 в качестве подтверждения истины документа. UI показывает происхождение оценки. Revision ordinal монотонен внутри объекта; глобального порядка по timestamp не обещается.

Новые canonical revision links записываются при той же транзакции, что и revision. Старые memory_sources не связываются с ревизиями по близким created_at: результат миграции для них — `legacy_unattributed`. Их можно показывать только как исторические источники уровня памяти, а не доказанную основу конкретного изменения.

Imported provenance хранит foreign identities как аннотации. Они никогда не выбирают локальный workspace, actor, policy, key или authority.

## 2.3 Предлагаемая реляционная схема

Физические имена:

- `memory_workspace_epochs(workspace_id PK, version bigint)` — invalidation epoch для browse cursors; не source of knowledge.
- `memory_revision_source_links(workspace_id, memory_id, revision_id, event_id, source_revision_id nullable, origin_kind, actor_id, created_at)`; PK `(workspace_id,revision_id,event_id)`; composite FK до MemoryRevision и Event в том же workspace.
- `source_collections(workspace_id,id,name,version,active_import_id nullable,created_at)`; unique `(workspace_id,id)`.
- `knowledge_sources(workspace_id,id,collection_id,external_id,relative_path,state,version,current_revision_id,created_at)`; unique `(workspace_id,collection_id,external_id)`.
- `source_revisions(workspace_id,id,source_id,revision_number,payload_material_id,payload_intent_id,format,classification,capture_event_id,recorded_at)`; unique `(workspace_id,source_id,revision_number)`; payload и capture неизменяемы.
- `memory_source_bindings(workspace_id,memory_id,source_id,source_revision_id,last_import_memory_revision_id,manual_override,version)`; в первой версии unique `(workspace_id,source_id)` и `(workspace_id,memory_id)`.
- `source_import_operations(workspace_id,id,principal_id,mode,state,version,collection_id,base_collection_version,preview_revision,expires_at,accepted_policy_version,created_at)`.
- `source_import_items(workspace_id,id,operation_id,ordinal,external_id,payload_material_id,base_source_revision_id,base_memory_revision_id,disposition,state,applied_revision_id,conflict_id)`; unique `(workspace_id,operation_id,ordinal)` и external_id внутри batch.
- `source_conflicts(workspace_id,id,source_id,memory_id,base_source_revision_id,incoming_source_revision_id,manual_revision_id,version,state,resolution_event_id nullable)`; immutable три references; resolution append-only.
- `memory_export_operations(workspace_id,id,principal_id,state,version,format,expires_at,pinned_selection,result_material_id nullable,created_at)`.

У каждой дочерней таблицы composite FK содержит workspace_id. Все новые content/authority таблицы получают FORCE RLS, guarded ownership, запрет прямого runtime DML и только именованные scoped entrypoints. Для новых полей с замкнутыми множествами SQL CHECK сверяется с domain enum тестом. В schema не появятся raw DEK, bearer token, глобальный plaintext hash или полный content в audit/outbox.

Хранение source payload — только material reference, не новый plaintext `content` столбец. Effective memory content остаётся в нынешнем memory persistence path и его поддерживаемом security envelope; это расширение не объявляет автоматически зашифрованной всю историческую memory storage. MW-00 обязан проверить поддержку выбранных labels всеми используемыми путями. Label, не поддерживаемый безопасно, запрещён для нового сценария.

## 2.4 Атомарная mutation

В новом baseline доступны `GovernedMutation<T>` и `commit_in(&mut dyn UnitOfWork, ...)` [S07]. Конкретный memory writer MUST работать с caller-owned UoW. `save_memory_with_revision` нельзя вызвать как вложенную commit-операцию из этого writer.

Последовательность для correction/import item:

1. Разрешить identity из доверенного authentication и выполнить доступные предварительные проверки.
2. Открыть scoped transaction с существующим InstallationMutationPermit.
3. Проверить/заблокировать canonical idempotency identity; точный committed replay возвращает прежнюю receipt **до** повторной CAS-проверки состояния, но **после** повторной авторизации доступа к результату.
4. В стандартном порядке взять configuration/policy guards, collection/source/binding при наличии, затем memory; идентификаторы одного класса сортировать. Внутри этой ветки не вызывать provider/embedding/vault.
5. Повторить authorization, classification и expected revision/state проверку под актуальными guards. Зафиксировать capture/correction Event, MemoryRevision, MemorySource, точный revision link, active head, search projection, audit, outbox и receipt в одной транзакции через shared authority.
6. Убедиться, что транзакционный DB trigger увеличил browse epoch; не увеличивать его повторно вручную. Commit.
7. После commit отдавать receipt; дальше существующий outbox запускает derived обработку.

Порядок guards должен быть согласован со всеми действующими writers во время MW-00/MW-02. Этот документ не разрешает переставить installation/material/provider guards ради удобства импорта. При конфликте порядков меняется план до кода, а не выполняется транзакция с потенциальным deadlock.

## 2.5 Идемпотентность

Новый HTTP client key — UUID. Server storage key namespaced по операции, authenticated principal и workspace; target входит в fingerprint. Старые routes сохраняют прежний wire contract и имеют явный compatibility adapter. В ключ не включаются attempt_id, случайный event UUID или server-generated revision ID.

Semantic fingerprint строится из нормализованной закрытой схемы: omitted vs null явно определены, порядок массива источников и selected item ids значим либо canonical sorted согласно API. Случайные UUID результата и ciphertext не входят в equality. Для нового sensitive content использовать существующий approved keyed commitment; никакой публичный SHA-256 содержимого не становится lookup API. Ошибка сериализации — ошибка, не hash пустой строки.

Replay при изменённом теле — 409. Replay exact после потерянного HTTP response — одна и та же canonical receipt, без новых revision, audit success или outbox. Незавершённый конкурентный запрос ожидает ограниченно либо получает retryable busy; он не начинает независимую mutation. Receipt удерживается как минимум пока операция/экспорт/активный retry может ссылаться на неё. Срок старых legacy keys не продлевается молча.

Для import delivery identity фиксирована `(operation_id,item_id,semantic input)`: два OutboxDispatcher не могут применить item дважды. Если commit состоялся, а outbox ack потерян, следующий handler читает durable item receipt и возвращает success без повторных side effects.

## 2.6 Read consistency

Detail читает active pointer, revision и её разрешённые источники согласованно. Разрешение видеть memory metadata не автоматически разрешает исторический content; проверяется каждая revision и каждый раскрываемый источник. Изменение прав до точки linearization запроса должно учитываться; уже отправленные клиенту байты отозвать невозможно.

Browse uses keyset `(created_at,id)` и cursor, bound к principal/workspace/filter hash/epoch/policy version. При изменении epoch или policy следующий page получает CURSOR_EXPIRED, UI начинает новую выборку. Не обещается snapshot через несколько минут открытой транзакции. Cursor opaque, защищён от изменения, не содержит скрытых source IDs открытым текстом. Total count отсутствует, если его безопасная семантика не доказана.

History pins максимальную видимую revision на первой странице, но заново проверяет права на каждом запросе. History восстановимого знания ограничена retention: удалённое содержимое не возвращается из cached receipts или export copies.
