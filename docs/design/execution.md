# Исполнение, внешние эффекты и восстановление

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Run и процесс — разные вещи

Run является предметным выполнением. Worker — процесс, который исполняет доступную работу. Завершение процесса с кодом 0 не является доказательством успеха всех Run. И наоборот: idle worker не означает ошибку сохранённого Run.

HTTP lifecycle-команды и протокольные адаптеры должны выражать переходы одной канонической истории. Проекция AG-UI/A2A или клиентская карточка задачи не может самостоятельно назначить результат внешнему эффекту.

## Точки неопределённости

Перед вызовом сохраняется намерение и проверяются полномочия. После вызова должен появиться относящийся к нему результат. Сбой между отправкой и сохранением receipt оставляет отдельную задачу установления исхода. Отсутствие ответа не доказывает, что провайдер ничего не сделал.

Для повтора нужны свойства конкретной операции и внешнего сервиса. Подтверждение риска повторного списания является отдельным авторизованным действием, а не автоматической веткой retry. Компенсация оформляется новым эффектом; она не стирает прошлое и не является SQL rollback внешнего мира.

## Provider identity

В целевом и частично реализованном governed path запрос связан с ревизиями connection/model, policy и ModelRequestEvidence. Замена процесса не должна незаметно выбирать другого провайдера по текущим environment variables, если выполнение уже приняло immutable binding.

Точная доступность каждого пути определяется пакетом и его evidence. Документация текущего endpoint не утверждает production-ready поведение всех состояний из domain enum.

## Граница P04/14D

Опубликованный финальный verdict принимает delivery-only ResultPrepared. Binding provisional keys, Live publication, продвижение corpus/generation, job success и worker closure из него не следуют. 14E описан как следующий предложенный контракт. Это не косметическое переименование статуса.

## Практическая диагностика

Вначале выяснить, что уже записано: намерение, dispatch, acknowledged response, preparation или publication. Не очищать историю и не пересоздавать job только потому, что интерфейс показывает ожидание. Если допустимое действие нельзя вывести из durable evidence, остановить автоматическое исправление и сохранить наблюдения для review.

---
**Основание:** [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [S01: docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
