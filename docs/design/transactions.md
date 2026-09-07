# Транзакционные границы и повтор запросов

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Какая атомарность уже наблюдается

PgMemoryRepository записывает memory, новую revision, source и search projection одной транзакцией. Для новой ревизии SQL проверяет ожидаемые content/state versions. Смысл этой границы — не опубликовать active memory без её содержимого и основания.

MemoryService после этого отдельно сохраняет outbox и idempotency. Поэтому нельзя называть атомарной всю последовательность только потому, что одна её часть атомарна. Сбой между частями и потеря ответа требуют отдельных проверок.

## Общий механизм

В application есть GovernedMutationApply и GovernedMutationRepository с `commit`/`commit_in`. Последний предназначен для уже открытой вызывающим транзакции: реализация не должна скрытно открывать и завершать вторую. Outbox/Idempotency/Audit также имеют методы участия в общей unit of work.

MW-02 проектирует единый receipt результата вместе с изменением, audit и заданиями на дальнейшую обработку. Это требование нового пути, а не утверждение, что текущий legacy writer ему уже соответствует.

## Идемпотентность

Ключ повтора относится к логическому запросу. Повтор с тем же ключом и иной семантикой должен конфликтовать. Случайные UUID, выделенные заново при HTTP-попытке, не должны превращать идентичный повтор в другую команду; текущий memory fingerprint специально исключает такие ID.

Проверка права выполняется в контексте действующего запроса. Старый idempotency receipt не даёт новому principal доступ к результату. Горизонт хранения ключа нужно описывать явно; бессрочная гарантия повтора не следует из cache с expiry.

## Outbox

Текущий контракт at-least-once: обработчик может повторно получить событие после сбоя между обработкой и подтверждением. Он обязан сходиться на той же предметной идентичности. Нельзя объявить exactly-once на основании имени очереди.

Неизвестный topic не превращается в processed. Delivery failure остаётся записанным, применяется backoff; исчерпание попыток даёт dead-letter, а не исчезновение обещанной работы. Payload должен иметь потребителя, а outbox не подменяет канонический event log.

## Lock order и внешние операции

Точный порядок захвата locks берётся из принятого пакета реализации. Во время медленного network/vault вызова нельзя без обоснования удерживать общую SQL-транзакцию. Разрыв между системами оформляется durable intent/witness и процедурой восстановления. Одна успешная половина не означает успех всего действия.

---
**Основание:** [S04: crates/vestrace-application/src/memory/ports.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/ports.rs), [S05: crates/vestrace-application/src/memory/services.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs), [S06: crates/vestrace-infrastructure/src/postgres/memory_repository.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-infrastructure/src/postgres/memory_repository.rs), [S07: crates/vestrace-application/src/governed_mutation.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/governed_mutation.rs), [S08: crates/vestrace-application/src/outbox.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs), [S09: crates/vestrace-application/src/idempotency.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/idempotency.rs).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
