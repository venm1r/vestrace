# 06. Экспорт и повторный импорт знаний

## 6.1 Назначение

Экспорт переносит ограниченный разрешённый набор памяти, исторических ревизий и происхождения. Это не backup PostgreSQL, не восстановление vault, не перенос identities, capabilities, tokens или qualification. Данные, уже скачанные внешним клиентом, нельзя отозвать удалением исходной memory.

JSON — машинный формат `vestrace.memory-package/1`. Markdown — человекочитаемое представление, которое не обещает lossless roundtrip. ZIP upload не поддерживается; пакет — один UTF-8 JSON, чтобы не вводить archive traversal/decompression угрозу в первой версии.

## 6.2 Снимок и права

ExportRequest явно перечисляет 1..100 memory_ids, include_history и format. Server фиксирует selection ревизий в согласованном scoped read и current principal. Пакет ограничен 8 MiB uncompressed. Превышение — typed failure, не обрезанный «полный» пакет.

Операция зафиксирует канонические selected refs и policy version. Outbox topic `memory.export.prepare` генерирует результат через existing protected material storage. Нельзя сохранять plaintext export в `/tmp`, public artifacts или CDN. GET content заново проверяет доступ к **каждой** pinned memory/revision/source; если доступ был отозван, весь результат withheld и существующий result material retire-ится. Не отдавать прежний downloaded_url, минующий policy.

Блокировка выдачи относится к моменту проверки и линейной отправке ответа; невозможно гарантировать отзыв байтов, уже отправленных клиенту. При streaming каждый chunk не становится новой независимой авторизацией; для небольшого MVP формировать bounded response после единой проверки. Disconnect не означает, что клиент ничего не получил.

## 6.3 Состав JSON package

Top-level: schema_version, package_id, generated_at, memories, sources, revision_links, completeness. Foreign UUIDs сохранены как IDs внутри package namespace. Каждая memory содержит kind, selected active_foreign_revision_id и разрешённые revisions. Каждая revision имеет content, declared classification, original timestamps, change_reason и provenance annotations.

В package нет active tokens, ключей, server credentials, raw request logs, permissions, key IDs, qualification verdicts и готовых vectors. Неизвестные поля такого рода запрещены schema. Plaintext content уже раскрыт получателю по export policy; весь файл следует считать чувствительным согласно наиболее ограничительному набору исходных obligations.

При недоступных provenance relations не включать запрещённые IDs/имена; completeness.provenance=`partial`. `completeness.provenance=complete` относится только к closure выбранных ревизий и не означает full history. При неизвестных/скрытых origins ставить partial. `history=selected_current` совместим с complete provenance для текущих выбранных ревизий. Будущая проверка цифровой подписи происхождения не является частью v1 и не превращает файл в доверенный issuer.

## 6.4 Импорт package

Тот же preview/apply pipeline имеет `mode=portable`. Выполняются schema/version/limits/references проверки, затем пользователь выбирает target collection и допустимую target classification. Полномочия определяются локально; imported metadata не даёт grant и не снижает label автоматически.

Локальные memory/source/revision IDs выделяются заново; immutable import mapping связывает package_id + foreign_id с новым local_id в рамках **конкретной операции**. Повтор той же операции возвращает то же mapping. Новое независимое перенесение того же package требует нового явного намерения и показывает duplicate warning, а не молча перезаписывает прежний target.

История импортируется с `recorded_at` локального импорта; `origin_recorded_at` сохраняет заявленное время отправителя. Нельзя вписать foreign actor в локальный Audit как действовавшего пользователя. Импортёр является текущим actor; прошлый actor — неподтверждённая origin annotation. Active означает выбранную версию в новой библиотеке, а не подтверждение истинности.

All intra-package links проверяются до apply. Cross-item ссылки записываются только после существования обеих локальных сторон и остаются pending/unresolved в промежутке; не выдавать полный provenance до закрытия набора. Для MVP сначала import memories/sources с mapping, затем второй idempotent linking pass через тот же outbox. Batch partial отражается в отчёте, не скрывается.

## 6.5 Retention и delete

Default expiry export material — 24 часа, после чего content endpoint возвращает 410 и запускает existing erasure protocol. История операции сохраняет безопасные IDs/исходы в пределах действующей retention policy, не копию содержимого.

Поступившее lawful erasure требование должно инвалидировать staged previews, готовые exports и derived context snapshots, которые содержат эти данные. Обновление версии разрешений не отзовёт локальные копии пользователей; это прямо указано в UI. Нельзя ради удобства replay держать запрещённый content в idempotency response_payload.

## 6.6 Приёмка

Export → preview target → apply → read result сохраняет content выбранных ревизий, разрешённые связи и annotation времени. Local IDs, actor и grants отличаются предсказуемо. Тесты сравнивают граф и content, а не исходные server IDs. Изменение label без разрешённого отображения, неизвестная версия, лишние secret поля, слишком большой пакет и недостающие refs дают refusal до canonical apply.

## Уточнение количества элементов portable import

Portable import использует максимум 100 canonical entities суммарно (memories + sources), 64 KiB на каждую content revision и 8 MiB UTF-8 во всех content значениях пакета. Полный wire JSON также ограничивается transport лимитом; дубли text fields не освобождают от decoded лимита. В таблице import items одна entity соответствует одному item; foreign revision mappings выделяются при фиксировании item, а links закрываются в финальной фазе того же `memory.source_import.apply`. Phase хранится в операции, не создаёт второй scheduler/state engine.

Предложенная таблица `portable_import_mappings(workspace_id,operation_id,package_id,foreign_kind,foreign_id,local_id)` имеет unique foreign tuple и unique local_id для данного kind/operation. Item diagnostics для memory entities используют синтетический отображаемый locator `portable/memory/<foreign_id>`, который не является путём чтения/записи диска.

`keep_manual` при sync фиксирует audit/binding resolution и возвращает MutationReceipt с прежней memory revision; создание пустой текстовой revision не требуется. При импортировании foreign history её номера могут содержать пропуски из-за selection; local revision sequence создаётся заново, origin number сохраняется как аннотация.
