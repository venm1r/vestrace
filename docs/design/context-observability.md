# Проект инспектора контекста и объяснений

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемая функциональная спецификация F205.

## Какие факты различать

Vestrace может фиксировать выданный им context. Клиент может сообщить, что использовал его. Доверенный integration hook может наблюдать окончательный model request. Это три разных вида evidence. Ни matching ID, ни client boolean не являются независимым наблюдением передачи модели.

## Представление

Query intent/time/scope → разрешённые кандидаты → selected revision set → rendered sections/budget → ограниченные warnings → optional observed request. Для каждого перехода видны source identity и версия обработки. Debug view проверяет права до показа, включая counts и deleted/withheld metadata.

## Сравнение экспериментов

Сравнивать два context request с одинаковым corpus snapshot, user scope и budget. Показывать добавленные/исключённые permitted revisions и стоимость. Изменение модели без сохранения версии/tokenizer нельзя объявить чистым улучшением retrieval.

## Хранение и воспроизводимость

Raw prompt/context может быть чувствительным материалом. Где допустимо, сохраняется bounded rendered payload под governed lifecycle; в остальных случаях — безопасные структурные identity с обозначенным пределом replay. Отказ хранить sensitive content не должен подменяться ложной гарантией полного воспроизведения.

## Порог готовности

На контрольном позднем исправлении оператор видит, какой источник стал новым основанием и почему старый контекст отличается. Неавторизованный caller не узнаёт content через trace API. Внешний runtime без observed request не получает отметку «модель точно увидела». Retention/erasure не обходятся forensic UI.

---
**Основание:** [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [S11: crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs), [S12: crates/vestrace-application/src/retrieval/context_builder.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/retrieval/context_builder.rs).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
