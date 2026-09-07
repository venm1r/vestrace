# Повседневная эксплуатация

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Начало наблюдения

Сначала определить точный build/configuration target и безопасный workspace. Проверить процессы и DB/schema readiness, затем backlog и состояния интересующих Run. Сохранять request/job/effect/revision IDs, не тела секретных запросов. Не начинать с удаления failed rows или restart всех компонентов без понимания результата внешнего действия.

## Ожидание и зависимость

Requested, Running, ResultPrepared и Ready generation относятся к разным этапам. Ожидание ключей/провайдера/полномочий не превращается в definite failure по elapsed timeout без соответствующего durable evidence. Показать оператору разрешённое действие или конкретный missing prerequisite.

## Outbox

At-least-once допускает повтор после processing commit до acknowledgement. Обработчик обязан быть идемпотентным по предметной идентичности. Неназначенный handler оставляет сообщение pending; его нельзя удалить как «мусор» только для уменьшения backlog. Dead-letter содержит причину и требует диагностируемого решения.

Новый topic добавляется вместе с реальным потребителем и тестом; outbox не используется как второй общий event log.

## Worker --once

Exit 0 — один cycle обработал работу; 3 — idle; 1 — ошибка poll/delivery. Успешная обработка work item может быть законным отказом/retry/dead-letter решением, а не успешной бизнес-задачей. Внешний scheduler должен трактовать эти коды по контракту, а не приравнивать ненулевой idle к corruption.

## Перезапуск

Сначала проверить, безопасен ли restart и сохранены ли persistent roots. После restart наблюдать recovery phase и pending debt. Не выдавать сбой response за permission to retry внешнего эффекта. Критические manual действия требуют отдельного grant/approval согласно операции.

## Инцидент

Сохранить safe evidence и ограничить затронутые функции. Исправление причины, recovery данных и возвращение опасных полномочий — разные решения. До повторной квалификации не заявлять прежний trusted статус только по успешному health probe.

---
**Основание:** [S01: docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md), [S08: crates/vestrace-application/src/outbox.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs), [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R03: crates/vestrace-cli/src/main.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-cli/src/main.rs).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
