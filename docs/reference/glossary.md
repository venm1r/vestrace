# Словарь проекта

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

| Термин | Значение в этой документации |
| --- | --- |
| **Memory** | Стабильная identity долговременной записи; не равна доказанной истине. |
| **MemoryRevision** | Конкретное неизменяемое содержимое/metadata записи. |
| **Source** | Основание происхождения; его наличие не гарантирует истинность. |
| **Claim** | Явное утверждение, когда требуется отдельная смысловая модель. |
| **Provenance** | Связи с источниками и преобразованиями; не автоматическое доверие. |
| **Canonical state** | Авторитетное состояние своей предметной области. |
| **Projection** | Перестраиваемое представление; не вправе переписывать основание. |
| **ContextPack** | Governed представление контекста с ограничениями и provenance. |
| **Classification label** | Метка vocabulary; порядок severity нельзя придумывать. |
| **Sensitivity** | Отдельное измерение чувствительности канала/материала. |
| **Capability** | Ограниченное право операции в текущей policy. |
| **Run** | Каноническое выполнение; не то же самое, что процесс worker. |
| **Outbox** | Доставка нужной работы; at-least-once, не второй event log. |
| **Idempotency** | Сходимость повторов одной логической операции в пределах контракта. |
| **CAS** | Изменение только при совпадении ожидаемой версии. |
| **UNKNOWN** | Исход не установлен; не permission to retry. |
| **ResultPrepared** | Подготовленный durable результат; не обязательно Live/Succeeded. |
| **Generation** | Идентичность состояния производного индекса по соответствующему contract. |
| **Material** | Управляемое содержимое с lifecycle/key authorities. |
| **B/I/M** | Base imported content, Incoming source, текущая Memory; стороны sync decision. |
| **Qualification** | Принятие свойств по evidence на точном target, не общий комплимент качеству. |
| **Priority P0–P4** | Предложенный порядок продуктовых групп; не номера frozen packages. |
| **P01–P12** | Существующие пакеты full-v1 gate program. |
| **MW-00–MW-07** | Предлагаемая программа Memory Workspace; не дополнение P13+ по умолчанию. |
| **NOT_RUN_HERE** | В этой работе не выполнялась соответствующая runtime проверка. |

Словарь поясняет, но не заменяет exact type/нормативный contract. При различии источником смысла остаётся соответствующий Accepted документ.

---
**Основание:** [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
