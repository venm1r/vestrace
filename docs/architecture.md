# Архитектура Vestrace

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Пояснение архитектуры; нормативные требования сохраняются в specs/ADR.

## Основная модель

Vestrace сохраняет долговременное знание и управляет его использованием. Memory, исходные события, ревизии, связи и полномочия не принадлежат transient-сессии модели. HTTP, MCP и Console дают доступ к одной модели, а не поддерживают независимые копии истины.

| Слой | Ответственность | Что не должно переходить в него |
| --- | --- | --- |
| Domain | Сущности, допустимые переходы, типы identity/evidence | SQL, transport, секретные bytes в обычных DTO |
| Application | Use cases, порты, policy/effect orchestration | Прямое знание HTTP/React или локальных таблиц клиента |
| Infrastructure | PostgreSQL, vault, provider transport, адаптеры | Незаявленные права и вторая бизнес-семантика |
| HTTP/MCP/CLI | Аутентифицированные входы, типизация запросов, процессная композиция | Обход общего mutation/policy контракта |
| Console | Проекции и команды пользователя | Прямая запись БД, локальное присвоение trust/status |

Это карта ответственности, не утверждение полной реализации каждого целевого механизма.

## Каноническое и производное

Исходное событие фиксирует происхождение. Memory сохраняет identity, а MemoryRevision — конкретное содержимое. Claims и conflicts представляют смысловые утверждения и их расхождения там, где требуется такой контракт. Embeddings, поисковые документы, summaries и ContextPack — производные представления.

Производное представление можно перестроить из своих оснований, пока они законно сохранены. Оно не может самовольно исправить более авторитетные данные. Ссылка на источник объясняет происхождение, но не является самостоятельным доказательством истинности утверждения.

## Одна граница исполнения

Run остаётся канонической execution authority. Внешние адаптеры преобразуют команды и наблюдения на границе. Ни retry-обработчик, ни новый importer, ни Console не создают параллельный generic Task/Attempt runtime. Специализированное состояние операции допустимо, когда оно описывает предметный результат, а не становится вторым источником истины о выполнении.

## Как читать подробности

[Домен](domain-model.md) объясняет сущности; [память и время](design/memory-time.md) — ревизии; [retrieval](design/retrieval-context.md) — сборку контекста; [транзакции](design/transactions.md) — атомарность; [execution](design/execution.md) — результаты внешних операций; [материалы](design/materials.md) — отделение content и key authority.

## Brain–Face–Organ

Принятый системный слой разграничивает постоянную cognition, активное рассуждение, пользовательский/host интерфейс и заменяемые execution endpoints. Его существование в ADR не означает, что отдельный Brain runtime, Host Broker или Organ уже реализован. Угроза этой декомпозиции — дать одному из адаптеров независимые полномочия или историю; это запрещается архитектурной границей.

Подробный нормативный текст доступен через [индекс спецификаций](specs/README.md). Эта глава поясняет его, но не заменяет.

## Memory Workspace

Выбранное расширение добавляет удобный внешний цикл вокруг существующей памяти. Source snapshot не подменяется ручной правкой; importer и редактор сходятся на общей mutation boundary; export проверяет актуальные права. Новые определения и их ограничения находятся в одном [пакете](implementation/memory-workspace/README.md), не дублируются здесь.

---
**Основание:** [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R10: docs/adr/0001-memory-first-persistent-cognition.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/adr/0001-memory-first-persistent-cognition.md), [S04: crates/vestrace-application/src/memory/ports.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/ports.rs), [S07: crates/vestrace-application/src/governed_mutation.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/governed_mutation.rs).

[Карта документации](README.md) · [Состояние и ограничения](status.md) · [Реестр источников](maintenance/sources.md)
