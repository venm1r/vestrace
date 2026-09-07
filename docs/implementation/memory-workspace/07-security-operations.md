# 07. Безопасность, очистка и наблюдаемость

## 7.1 Threat model расширения

Рассматриваются злонамеренный MCP/HTTP caller, устаревший browser, два конкурентных writers, hostile imported Markdown/JSON, ошибочный local scanner, умирающий worker, неизвестный исход HTTP POST, отозванное право между preview и apply/download. Компрометация host root, PostgreSQL superuser и модели как универсального «здравого смысла» не решается этим пакетом.

Обязательная граница: identity получена trusted middleware. Workspace selection проверяется относительно токена. Worker не получает право пользователя из JSON и не использует собственный широкий principal для обхода чужого denied request. Collection name, path и source title могут быть чувствительными данными.

## 7.2 Content disclosure

List/detail/history/context/source diff/export используют единый `MemoryReadService`/policy projection. Проверки классификации делаются до передачи content ranker/model и до формирования snippets. Browser roles не являются enforcement. Исторический более закрытый label проверяется по собственной revision, а не current memory label.

Capabilities на уровне операции и read/destination predicates на уровне content должны быть выполнены одновременно. Missing/denied refs не раскрываются безопасными «подсказками» с чужими именами. Audit и metrics содержат operation IDs и агрегированные статусы только в разрешённом scope.

## 7.3 Storage boundary

Новые source/upload/export payloads идут через existing MaterialIntentCommands, существующий vault и erasure [S17]. Новый `SourcePayloadStore` лишь адаптирует эту authority к source input и не содержит собственную криптографическую state machine.

MW-04 должен доказать lawful обычный content owner, чтение и cleanup. Embedding-specific provisional keys из 14C/D для документов запрещены. Не создавать успех фиктивным VaultReceipt. Если существующий content owner/read API не позволяет нужную привязку, оформить явное расширение того же owner model в MW-04, с raw-SQL/runtime-role тестами, а не использовать другой store.

Ключи/receipts связываются с workspace и owner tuple. Host-vault операции не выполняются внутри БД-транзакции. Временное plaintext живёт только в bounded request/process buffer и очищается после завершения насколько это позволяет используемый тип; нельзя обещать полную zeroization JavaScript/OS buffers. Logger не принимает content body. Системные temp-директории не являются storage для staging.

## 7.4 Чувствительность и label changes

Label taxonomy не имеет автоматически выведенного порядка. Нельзя решить «internal < confidential» на основании строки, если принятый policy model этого не задаёт. Обычный editor и sync сохраняют нынешнюю classification. Другой label → отказ и отдельный governance workflow. Imported labels не исполняются как local policy.

No-secret default scan — предупреждение против случайности, не DLP guarantee. Для экспорта plaintext дополнительно требуется explicit actor intent и разрешённый destination. Скачанный JSON/Markdown получает ясное предупреждение о чувствительном содержимом.

## 7.5 Idempotency, retry и наблюдаемые исходы

В новом API Idempotency-Key scoped к operation/principal/workspace. Возврат старой receipt требует доступа сегодня. Body fingerprint никогда не строится из случайных result IDs; он не является открытым equality oracle.

`result-unknown` — допустимое клиентское состояние. После timeout нельзя показывать «ничего не сохранилось». Повтор с тем же body/key или чтение известной operation — правильное действие. Retry outbox доставляет запрос не менее одного раза, не гарантирует один вызов handler; canonical receipt/unique guards обеспечивают один бизнес-эффект [S08].

## 7.6 Эксплуатационные сигналы

Без raw текста: число PreviewReady/applying/blocked/failed, длительность staging/apply, возраст незавершённой операции, dead-letter items, projection pending и причины неподдерживаемого tokenizer/storage. Такие metrics сами не являются proof of correctness и не повышают state до Trusted.

У job есть ссылки на реальные item receipts и исходные тестируемые states. После restart оператор видит, что уже committed, что требует retransmit, что отменено и что blocked. Doctor/repair не получают отдельного пути DML: переиспользуются existing guarded commands.

## 7.7 Readiness и supported environment

До включения: migration/schema gate; narrow provisioner; ordinary material vault support; `memory.source_import.apply` и `memory.export.prepare` registered в настоящем worker; callback actor resolution; supported tokenizer только при заявлении token cap. Возможность write/edit зависит от approved shared mutation path, а Context/embedding readiness дополнительно от завершённых текущих generation boundaries.

Отключённая функция отображается как unavailable/not_enabled. Feature flag не разрешает обход security checks. Флаг не меняет значения legacy endpoint и не включает permissive defaults. Backup/restore/new content-owner compatibility проверяются отдельно от импорта знаний.
