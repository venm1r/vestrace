# Содержимое, ключи и материальные объекты

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Зачем отдельная модель

ContentMaterial описывает управляемое содержимое и lifecycle, а не просто набор bytes. Ключи, ciphertext, обычные ссылки и подготовленные вложения имеют разные полномочия. Наличие файла на диске не является разрешением читать его и не доказывает его публикацию.

Состояния подготовленного материала и Live различаются. Только соответствующая authority может завершить переход после необходимых receipts. Ни importer, ни exporter не должны объявлять подготовленный blob обычным доступным материалом.

## Два хранилища

Runtime material vault хранит защищённые ключевые материалы установки и должен сохраняться между перезапусками. Bootstrap secret store предоставляется отдельно, монтируется только для чтения и не создаётся Compose. В текущем deploy-контракте корни не должны совпадать или содержать друг друга.

Installation fingerprint — ещё одна постоянная identity. Резервная копия базы без необходимых vault-данных не является доказанной возможностью восстановить рабочую установку. Копирование секретного хранилища требует отдельной политики доступа, а не помещения ключа в ZIP с документацией.

## Восстановление через состояния

MaterialIntentResumption восстанавливает по persisted snapshot. Если содержимое не было подготовлено до гибели процесса, нельзя восстановить его догадкой; в предусмотренных случаях применяется явная ветка abandonment. Специализированные embedding-output intent имеют собственные ограничения и не должны попадать под обычный finalizer как обход.

Хостовый vault и PostgreSQL не имеют общей атомарной транзакции. Порядок durable markers/witnesses и exact replay поэтому относится к контракту, а не к удобству адаптера. Нельзя заменять его in-memory mutex или elapsed timeout.

## Удаление и производные данные

Удаление исходника, удаление производного индекса и уничтожение ключа отвечают на разные вопросы. Необходимо проверять удержание зависимостей и допустимость erase. Исторический audit сохраняет безопасные сведения, но не должен тайно сохранять копию стираемого plaintext.

Для MW source staging и export должны пользоваться существующим материалом, а не обходить его через временный открытый файл. Полная реализация этого пути является compatibility gate пакета.

---
**Основание:** [S17: crates/vestrace-application/src/material/commands.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/material/commands.rs), [R04: docker-compose.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml), [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [S01: docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
