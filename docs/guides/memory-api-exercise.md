# Практикум: записать событие, память и выполнить поиск

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Область сценария

Ниже показаны текущие структуры запросов из HTTP-кода. Предпосылки: уже настроенный локальный сервер, token с EventWrite/MemoryWrite/MemoryRead/ContextRetrieve и разрешённая policy. Выполнение этих запросов создаёт долговременные записи; используйте только синтетические данные в одноразовом workspace. Этот практикум не запускался в данной среде.

## 1. Сохранить происхождение

`POST /v1/events` с отдельным `Idempotency-Key`:

```json
{
  "event_type": "documentation.example",
  "payload": {"statement": "В учебном проекте принято решение использовать PostgreSQL."},
  "session_id": null
}
```

Из ответа сохраните `id`. Это event данного источника, не независимо доказанная истинность утверждения.

## 2. Создать память

`POST /v1/memories`, другой устойчивый ключ логического запроса:

```json
{
  "kind": "decision",
  "content": "Учебное решение: использовать PostgreSQL.",
  "confidence": 0.5,
  "importance": 0.5,
  "source_event_id": "10000000-0000-4000-8000-000000000003",
  "evidence_role": "direct_source"
}
```

UUID в примере — демонстрационный, его нужно заменить реально полученным event ID. Числа 0.5 — учебные допустимые значения, не измеренная вероятность истинности. Политика установки может требовать classification; передайте только разрешённое значение её vocabulary.

Успех сохранения ещё не доказывает готовность embedding/generation. Отдельно проверьте [границы P04](../status.md).

## 3. Получить метаданные

`GET /v1/memories/{id}` в этом baseline возвращает метаданные. Нельзя ожидать поля `content` или current revision number лишь потому, что они есть во внутреннем domain type. Новый detail/history контракт находится в [MW-01](../implementation/memory-workspace/plans/01-memory-read-context.md).

## 4. Поиск

`POST /v1/retrieval/search`:

```json
{
  "query": "Какую базу выбрали для учебного проекта?",
  "intent": "decision_recall",
  "limit": 10,
  "time_perspective": "current",
  "token_budget": 512
}
```

Проверьте кандидаты, exact revision refs, degraded/warnings. Текущий HTTP ContextPack — сводка, не готовый текст для модели. Отсутствие кандидатов не доказывает отсутствие знания: проверьте scope, состояние записи и доступность каналов.

## 5. Новая ревизия

Endpoint существует: `POST /v1/memories/{id}/revisions`; требуется числовой `If-Match` ожидаемой content revision и source event нового основания. Не угадывайте номер на действующей общей памяти: полноценный внешний read/edit loop ещё проектируется. На изолированном только что созданном примере первая content revision формируется как 1, но concurrent writer может изменить это до запроса; conflict нужно обрабатывать, не обходить.

## Итог наблюдения

Запишите commit, среду, версии, request IDs и безопасные результаты. Не фиксируйте токены/URL с credentials или пользовательский content. Успешный пример на одной среде не заменяет concurrency, restart, access-control и release tests.

---
**Основание:** [S10: crates/vestrace-http/src/api/memory.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs), [S11: crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs), [S05: crates/vestrace-application/src/memory/services.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs), [R01: crates/vestrace-http/src/auth.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
