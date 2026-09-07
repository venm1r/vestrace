# Как учтены приложенные документы

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Редакторская сверка входных материалов.

| Материал | Что сохранено | Что изменено при использовании |
| --- | --- | --- |
| Handbook RU на 58e7dac3 | Memory-first объяснение, разделение evidence, API/operations тематика | Устаревшие implementation observations не перенесены как состояние 07e2977 |
| MW Implementation на 6f610253 | Detailed requirements, 8 plans, schemas, fixtures, safety/integration limits | Canonical версия берётся из интегрированного дерева, не из раннего ZIP wrapper |
| Integrated docs и patch | Размещение MW, navigation authority boundaries | Вводные docs теперь переписаны; прежние integration reports остаются историческими |
| Монолитные Markdown | Удобное чтение тех же материалов | Не создают вторую authority; новая reading edition генерируется из файлов |
| Promotion plan | Demo/quickstart/pilots и цели внешней валидации | Текущие правила площадок/рыночные цифры не зафиксированы как вечные факты |
| Репозиторий 07e2977 | Фактическое наличие интеграции, кодовый ancestor и текущие public surfaces | Выбранные source observations отделены от непроверенных runtime outcomes |

Контрольные суммы входов находятся в [input-documents.json](input-documents.json). Название архива с прежним SHA не означает, что надо вернуть код назад. Сравнение 6f610253→07e2977 показывает документационный commit; версия runtime ancestors указана явно.

Полный MW subtree сохранён byte-for-byte; новые roadmap карточки отсылают к нему и не меняют его Proposed decisions, сценарии или candidate migrations. Старые manifest hashes проверяются в своей области, не переинтерпретируются как hashes обновлённых оглавлений.

Нормативные оригиналы и historical evidence не включены второй копией в overlay. Они доступны по прежним путям целевого repo и pinned ссылкам в нормативном/историческом разделах.

---
[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](sources.md)
