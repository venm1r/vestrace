# Workflow агентной разработки

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Ответственность

Использовать выбранную схему: **Sol — orchestrator/architect/reviewer → Terra — persistent builder → Sol — final quality gate**. Названия здесь обозначают согласованные роли workflow, а не требования к конкретной текущей модели или её лимитам.

Архитектор владеет требованиями, зависимостями и границами принятия. Builder реализует согласованный scope и показывает фактические результаты. Reviewer в отдельном контексте проверяет контракт, тестовые предпосылки и полный путь. Самоотчёт builder не является независимым review.

## Вход задания

Указать commit, точный список read/write/protected paths, требования с IDs, зависимости и критерии остановки. Дать ссылки на canonical spec и один план, не десяток копий с разными числами. Если появляется отсутствующий authority, не придумывать обход: оформить gap и предложенный amendment.

## Вопросы reviewer

Где production producer создаёт это состояние? Какие fixtures подменяют его? Как обходится guard старым endpoint? Как сходятся повтор и конкурирующая правка? Можно ли удалить реализацию и всё равно пройти тест? Какой смысловой пункт spec не связан с acceptance case?

## Качество процесса

Измерять не количество review, а найденные и escaped defects, объём rework, стоимость прогона и воспроизводимость. Переоткрытие P04 означает полезную способность исправлять вывод, но не доказывает, что harness никогда не пропустит пробел.

Не требуется отдельный новый tool-product для реализации этой дисциплины. Сначала сохранить её переносимой, краткой и пригодной для текущего проекта.

---
**Основание:** [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [S01: docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
