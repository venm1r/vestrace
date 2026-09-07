# Пробелы, зависимости и безопасные границы

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Блокеры для полного внешнего цикла памяти

Текущий detail-ответ не выдаёт содержимое, а retrieval не выдаёт текст ContextPack. Новые list/history/correction UI нужны не для украшения API, а чтобы клиент мог читать ровно то, что собирается исправлять. MW-01 решает эту поверхность; MW-02 — границу надёжной записи.

Атомарность memory/revision/source/search не означает атомарности всего бизнес-командного результата. В текущем сервисе outbox и idempotency следуют отдельными вызовами после сохранения памяти. Документация не обещает, что любые потерянные ответы безопасно повторяются без дополнительной проверки. Предложенное закрытие находится в MW-02; изменять код эта редакция не пытается.

## Что зависит от P04

14D фиксирует подготовленный результат и явно не публикует Live-векторы, не завершает job и не доказывает worker composition. Необходимо читать последний хвост evidence, а не ранний заголовок «complete». MW не должен обходить эти границы старым тестовым адаптером.

Отдельный текстовый путь можно принимать только как отдельно проверенный ограниченный путь. Отсутствие векторного результата нельзя маскировать утверждением «вся индексация готова».

## Установка и авторизация

Публичного рецепта bootstrap в этой поставке недостаточно для обещания запуска на совершенно чистой машине: корректный mounted secret store остаётся предварительным условием. Руководство показывает запуск после подготовки и диагностику, но не генерирует неподтверждённый формат ключа.

HTTP route inventory определяет поверхность отказа/допуска, но не успешность каждого handler. Особенно осторожно нужно обращаться с 501, незаполненными policy/qualification и библиотечными возможностями, не подключёнными в production root.

## Квалификация

Ни CI-конфигурация, ни результаты другой ветки, ни схемы JSON не доказывают готовность конкретной установки. Backup/restore, browser flow, права реального runtime и отказоустойчивость должны получить отдельные наблюдения. До этого нельзя позиционировать весь набор как production-qualified.

## Правило следующего изменения

Перед любой реализацией повторно проверить дерево, frozen-предшественников и scope. Документационные номера миграций не резервируют места в растущей истории. Новое улучшение не должно отменять сохранённое ограничение без результата, который действительно его закрывает.

---
**Основание:** [S05: crates/vestrace-application/src/memory/services.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs), [S10: crates/vestrace-http/src/api/memory.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs), [S11: crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs), [S01: docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md), [R04: docker-compose.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
