# Проверка интеграции документации

**Область:** связность Memory Workspace с существующими документационными входами. **База:** `6f6102536e9a535b7086db14573bf45fe750ad71`. Это не runtime qualification.

## Состав изменения

Изменены четыре существующих документа: корневой README, `docs/architecture.md`, `docs/specs/README.md`, `docs/plans/README.md`. Добавлены общий указатель `docs/README.md`, реестр `docs/implementation/README.md`, весь Memory Workspace и его контракт интеграции. Корневой README исходного ZIP не копируется поверх README проекта.

Frozen leaf specs, Accepted ADR, P01–P12, P04 scope/preflight, исторические evidence, `PLAN.md`, product code, runtime schemas, migrations и CI не входят в изменение. Существующие файлы реконструированы из GitHub и до редактирования сверены по четырём Git blob SHA-1; совпадение байтов подтверждено.

## Сохранность пакета

45 исходных требований, 23 задачи, 54 будущих acceptance cases, 41 schema definition и 20 proposed HTTP operations сохранены. Предметные главы и планы не перепроектированы. Уточнены только навигация и статус передачи; добавлена карта соответствия старой и новой документации.

Большой негативный пример UTF-8 представлен ограниченным декларативным fixture вместо 40 000 повторённых символов в файле. Валидатор разворачивает его в тот же JSON payload до schema/semantic проверки; равенство с исходным примером проверено. Никакие scripts из fixture не исполняются.

## Проверки и пределы

Валидатор пакета проверяет JSON, schema, references, примеры, Markdown-ссылки/anchors, Python syntax, traceability и манифест. Дополнительная локальная проверка сопоставляет новые навигационные ссылки с полученными GitHub targets либо локальными документами и сравнивает исходные карты требований с поставкой.

Подробные результаты: [validation-result.json](validation-result.json), [integration-result.json](integration-result.json), [integration-manifest.json](integration-manifest.json). Манифесты содержат контрольные суммы, но не подпись и не утверждение о доверии к коду.

Это редакционная самопроверка, не независимый архитектурный review. Исторические ссылки, которые уже присутствовали в старых индексах, не переаудированы целиком. Публичные внешние страницы не проверялись на свежесть. Rust/PostgreSQL/browser и прочие product tests не запускались.

## Повторить в полном checkout

```bash
python docs/implementation/memory-workspace/verification/validate_bundle.py
```

Проверку GitHub commit/tree и применимость patch нужно проводить относительно фактического checkout. Предлагаемые команды реализации нельзя засчитать как выполненные проверки этой поставки.
