# Оценка полезности памяти

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемая методика и синтетические fixtures.

## Что входит

[Синтетический корпус](corpus.json) содержит 8 учебных sources и 12 контрольных вопросов. Это тестовые ожидания, не результаты запуска и не факт о настоящем проекте. Все результаты `NOT_RUN`. Advanced historical-as-known случаи проверяют будущую цель F201; если текущий продукт не поддерживает её, следует сообщить unsupported, а не подменить смысл запросом current.

## Два слоя оценки

Сначала retrieval: разрешённые exact evidence refs, отсутствие denied bytes, актуальность и warnings. Затем answer: ответ по данным, отсутствие неподтверждённых утверждений и правильное понимание времени. Контекст, выданный сервисом, отделяется от реально наблюдаемого model request.

## Базовое сравнение

Один и тот же corpus snapshot, scopes, model revision и budget. Базовый вариант — простой FTS на том же разрешённом наборе. Затем полная конфигурация Vestrace. Без matching conditions нельзя приписывать улучшение конкретному memory механизму.

Security cases — детерминированные запреты, не шкала model judge. Для содержательных ответов можно использовать размеченные expected behaviors и независимый review. Число совпадений само по себе не доказывает качество источника или safety.

## Запись эксперимента

Указать dataset digest, code/config/model/tokenizer identities, hardware, repetitions, raw safe observations и exclusions. Не переписывать предыдущий run после изменения corpus. Для малого числа случаев сообщать числитель/знаменатель и сами ошибки, а не убедительную «точность 99%».

## Gate

Новая конфигурация не может пройти при утечке scope, silent overwrite или ложной certainty после crash, даже если средняя helpfulness выросла. По регрессии выбирается задача P0/P1/P2; автоматически добавлять новые retrieval channels не нужно.

---
**Основание:** [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [S11: crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
