# 05. Импорт, повторная синхронизация и конфликт с редактором

## 5.1 Форматы и пределы

Вход документов: JSON envelope `ImportPreviewRequest` с `mode=documents`, collection_id, base_collection_version, scan_kind и documents. Каждый document имеет external_id UUID, relative_path, format, content, classification и явные confidence/importance. Markdown и text хранятся как точная строка UTF-8; не выполняется извлечение semantic claims и не исполняется frontmatter.

Лимиты: 1..100 documents; один content 1..65 536 UTF-8 bytes; суммарно до 8 MiB декодированного content. Byte checks выполняются сервером, а не только JSON maxLength. BOM и недопустимый UTF-8 отклоняются; CRLF/LF сохраняются в snapshot, не маскируют изменённый источник. Unicode/path handling не нормализует содержимое. Повторная отправка того же текста означает те же UTF-8 bytes.

JSON package переносимости имеет mode=portable и отдельную schema, не произвольный документ, который модель должна угадать. Unknown fields, duplicate JSON object keys и dangling internal references отклоняются до canonical apply. Parser сохраняет bounded size, depth ≤ 32 и число nodes ≤ 20 000; NaN/Infinity невозможны в JSON.

## 5.2 Локальный scanner

Предлагаемая CLI группа: `vestrace sources scan|preview|apply|status|export`. Она является клиентом HTTP, не получает прямой database URL и не запускает второй сервер. `scan` требует явный `--root` и `--state-file` вне сканируемой папки; новые UUID сохраняются через temporary file + atomic replace локального mapping. Mapping не источник серверной authority.

Scanner обходит только regular files с разрешёнными suffix. Symlink, junction/reparse point, path traversal, absolute/UNC/drive path и escape за root отклоняются; изменение файла во время чтения даёт retryable scan-changed, не смешанный snapshot. Полагаться только на предварительный realpath нельзя: открыть handle без following links и проверить объект перед/после чтения на поддерживаемой платформе. Если безопасный способ для конкретной OS не реализован, CLI отклоняет этот режим, а не молча снижает защиту. Browser upload не читает server filesystem.

По умолчанию исключаются .git, .env, private-key/credential files, node_modules, target, hidden directories и любые внешние URLs. Это защита от случайной загрузки, не гарантия отсутствия секретов в обычном .md. Пользователь видит точный список и подтверждает право загрузить данные. Аудит не сохраняет абсолютные пути.

`partial` scan сообщает только перечисленные файлы. `complete` — заявление доверенного клиента, что весь выбранный набор просмотрен без ошибки. Отсутствие прежнего external_id в полном scan создаёт Missing observation; автоматически удалять source/memory нельзя. Результат частичного/ошибочного scan не обозначается complete.

Rename: тот же external_id и новый locator сохраняют source identity. Если после переименования scanner не может надёжно сопоставить ID, он предлагает явное mapping. Сравнение content hash не доказывает rename; два одинаковых файла остаются разными sources.

## 5.3 Безопасный staging

`POST /v1/source-imports` сначала валидирует envelope, текущие права и supported storage labels. Затем резервирует operation/input identities с idempotency и сохраняет payload через новый narrow `SourcePayloadStore` — consumer существующих MaterialIntentCommands [S17]. Он не использует embedding_output key operations и не объявляет generic vault callback готовым без проверки.

Материал проходит существующий reserve → ciphertext preparation → bind → Live protocol. Vault/файловые операции выполняются вне PostgreSQL transaction. В БД находятся typed material references, receipts и метаданные; outbox и audit содержат только IDs. Потерянные до ContentPrepared байты не восстанавливаются из памяти умершего процесса. Такой upload требует повторной передачи: previous intent сначала корректно retire через existing authority, новый attempt связан с тем же operation, PreviewReady не выставляется.

Успешный response 201 PreviewReady возможен только после durable сохранения **всех** одобренных input snapshots. Если HTTP response потерян, exact replay возвращает уже готовую operation/preview. Partial staging не создаёт active memory и не становится successful preview; status отражает staging/needs_upload. Пользовательское тело повторно используется только при совпадении semantic request tuple.

Истечение preview через 24 часа (предлагаемый default) переводит его в Expired и инициирует lawful retirement staged material. Время не доказывает отсутствие живого vault writer: expiry fence и material lifecycle должны победить продолжающийся writer до erasure. Staging endpoints не читают arbitrary path/URL, не загружают npm dependencies и не вызывают LLM.

## 5.4 Preview

Preview фиксирует: operation_id, preview_revision, collection configuration version, current policy version, immutable inputs, base source head, base memory head/state revision и disposition по item. Dispositions: `new`, `update`, `unchanged`, `rename`, `conflict`, `missing`.

- New: external_id отсутствует в текущей collection.
- Unchanged: exact source content/format/label и locator не изменились. Случайный import operation UUID не создаёт новую source revision.
- Rename: изменился только locator, identity stable. История locator фиксируется как metadata change; content memory revision не создаётся без изменения content.
- Update: source изменился, effective memory всё ещё соответствует последнему accepted import и manual_override=false.
- Conflict: source изменился при manual edit/override, либо semantic choice небезопасно вывести из сохранённых versions.
- Missing: explicit observation полного scan, не операция удаления.

Смена classification для существующей source-backed memory отклоняется с CLASSIFICATION_TRANSITION_REQUIRED; она не является обычным update. При необходимости пользователь должен применить отдельный принятый governance workflow, которого этот пакет не добавляет.

## 5.5 Подтверждение и применение

Apply принимает exact preview_revision и список item_ids. Он заново проверяет права, expiry, configuration и base collection version. В одной транзакции reserve active_import_id коллекции, freeze выбранный item set, audit/receipt и по одному outbox сообщению `memory.source_import.apply` на изменяемый item. Topic не выпускается без смонтированного обработчика.

Preview не даёт необратимого разрешения на будущие байты. Иные входы требуют другой preview. Если collection изменилась до Apply — PREVIEW_STALE без частичного приёма. На одну collection действует один applying batch; manual edits разрешены и обрабатываются per-item CAS.

Обработчик использует контекст инициатора операции, заново разрешённый из durable identity и текущих grants. Worker own ExecutionWrite не повышает полномочия первоначального пользователя. При revoked permission item получает blocked_policy outcome; остальные уже committed items не отменяются. Service делает per-item canonical transaction через shared commit_in.

Atomic item включает capture Event, immutable SourceRevision, Memory/Revision/SourceLink, binding, search projection, audit, exact item receipt и update progress. Source head не указывает на недописанную revision. Batch не держит одну транзакцию на всё и может быть completed_with_issues.

После commit до outbox ack процесс может умереть; повтор видит item receipt. Error в pre-commit не оставляет новую memory/source head. Unchanged item имеет результат наблюдения, но не canonical revision и не embedding request. Index invalidation отправляется через принятый общий путь, без legacy raw-provider shortcut. При отсутствии завершённого поддерживаемого indexing path readiness остаётся pending/blocked.

## 5.6 Статусы

Operation: `staging`, `needs_upload`, `preview_ready`, `applying`, `completed`, `completed_with_issues`, `cancelled`, `expired`. Это business progress, не новая модель исполнения.

Item: `staged`, `queued`, `applied`, `unchanged`, `conflict`, `missing`, `skipped`, `blocked_policy`, `failed`. У applied есть immutable canonical receipt. Retryable transport/storage failures остаются ответственностью существующего outbox; исчерпание попыток отражается как failed + dead-letter evidence, а не бесконечный pending.

Index states отдельно: `not_required`, `pending`, `ready`, `blocked`, `failed`. Ready обосновывается фактическим qualified publication, а не количеством drained messages. `worker --once` code 0 означает работу в цикле, не успешный весь import.

Cancel берёт operation/collection guard. После cancellation fence новые item transactions не применяются; уже начавший canonical commit либо побеждает целиком, либо rollback. Уже applied content не уничтожается и не откатывается. Необработанные элементы становятся skipped; итог отчёт указывает applied subset.

## 5.7 Трёхсторонняя синхронизация

B = accepted source revision; I = immutable incoming source revision; M = current effective memory revision.

| Изменился источник | Есть manual edit/override | Решение |
|---|---|---|
| Нет | Нет | Unchanged |
| Нет | Да | Сохранить M, не создавать конфликт из-за одного повторного scan |
| Да | Нет | CAS применяет I как новую effective memory revision |
| Да | Да | SourceConflict; B/I/M сохраняются, M не изменяется |

`accept_source`: новая memory revision с I, manual_override=false, source binding advances.  
`keep_manual`: M сохраняется; отдельный resolution Event связывает override с I, manual_override=true. При следующем source change возникает новая необходимость проверки.  
`merge`: пользователь задаёт новый текст; новая revision с B/I/M provenance, manual_override=true. Это human-authored merge, не детерминированное восстановление.

Все три команды сравнивают expected_conflict_version и exact current memory/state + incoming source references. Любая смена basis после UI preview — 409. Решение не обновляет I, не удаляет B и не превращает source timestamp в authority.

Для первой версии допускается только один открытый conflict на source. Новый sync того же source при открытом conflict возвращает conflict_pending и не supersede-ит основания автоматически. Пользователь сначала решает текущий конфликт или явно отменяет его через принятую процедуру; это ограничение предотвращает ложные двухсторонние merges.

## Уточнение границ preview

Для complete scan максимум 100 preview items относится к объединению входных и потенциально Missing источников. Если это объединение больше лимита, сервер отказывает INPUT_LIMIT_EXCEEDED и предлагает bounded partial scans; он не усекает список и не делает вывод Missing по неполному snapshot. Пустой complete scan требует explicit empty confirmation через отдельный будущий contract и в MVP не выполняется автоматически.

Первый MW-04 допускает new/unchanged initial imports; изменение уже существующего source включается только после принятия MW-05. UI показывает not_enabled для такой sync, а не использует прямую update ветку. В PreviewReady решения immutable; repreview создаёт новое намерение с новым key.

## Правка памяти, пока конфликт уже открыт

`source_conflicts.manual_revision_id` — M0 на момент обнаружения; он остаётся неизменным. GET ConflictDetail возвращает `original_manual_revision` (M0), `manual_revision` (разрешённый текущий M) и текущий `memory_state_revision` в одном согласованном чтении. Поэтому дополнительная правка пользователя не делает конфликт навсегда неразрешимым: UI показывает отличие M0/M, пользователь подтверждает решение с актуальными expected_memory_revision_id/state. Original B/I/M0 не переписывается, resolution event содержит фактически использованный M.

Обычная correction связанной памяти атомарно устанавливает `MemorySourceBinding.manual_override=true` и продвигает binding version в том же writer/UoW. Это авторство определяется маршрутом/типизированной application-командой, а не пользовательским полем `origin_kind`. Import writer обновляет last_import_memory_revision_id и не изображает импорт ручной правкой. Explicit conflict resolutions обновляют binding согласно своему решению.
