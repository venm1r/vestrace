# 00. Актуальный срез кода и карта разрывов

**Baseline:** `6f6102536e9a535b7086db14573bf45fe750ad71`; parent `58e7dac3cef7cc106d59f7ba3560bdbd37de76c0`. Commit от 2026-09-07: `feat(p04): advance reopened embedding lifecycle`. Прочитаны выбранные исходники и документы через GitHub; локальная рабочая копия автора и runtime не проверялись.

## 0.1 Что изменилось относительно предыдущего обсуждения

[S01](sources.md#s01) содержит принятый `worker --once` (0 — выполнена работа, 3 — idle, 1 — ошибка), завершённый блок 14B пред-dispatch termination и финальное принятие 14D. 14D доводит delivery до durable ResultPrepared; он не объявляет Live-публикацию, Succeeded, завершение rebuild/retrieval-query или composition worker. В конце прочитанного участка 14E описан как proposal.

Следовательно, этот пакет **не предлагает повторно делать 14B–14D**, не занимает номер 0195, названный 14E, и не объявляет векторный pipeline законченным. Функции чтения/редактора могут разрабатываться отдельно; доступность поискового контекста проверяется через реальный RetrievalService, без обхода generation/policy guards.

## 0.2 Наблюдения

| ID | Подтверждено статическим чтением | Следствие для реализации |
|---|---|---|
| GAP-01 | `MemoryResponse` — id/kind/status/classification/dates; content и current revision отсутствуют [S10] | Добавить отдельный безопасный detail DTO, не менять молча старый ответ |
| GAP-02 | `ContextPackDto` возвращает счётчики, но не секции [S11] | Экспонировать реально построенные разрешённые секции новым endpoint |
| GAP-03 | `MemoryUseCases` имеет record/remember/revise/find, но не list/history [S03] | Добавить query port; не собирать библиотеку серией N запросов из UI |
| GAP-04 | Repository атомарно сохраняет память/ревизию/источник/поисковую проекцию, service затем отдельно пишет outbox и idempotency [S04–S06] | Новый editor/import обязан включать всё в caller-owned UoW |
| GAP-05 | В новом baseline есть `GovernedMutationRepository::commit_in` [S07] | Переиспользовать существующую transaction authority, не оборачивать commits во вторую транзакцию |
| GAP-06 | Fingerprint памяти уже исключает случайно выделяемые ID [S05] | Не записывать старый исправленный дефект как новый; проверить race/replay на сервере |
| GAP-07 | У idempotency есть save_in, запись scoped по workspace [S09] | Добавить operation/principal ownership результата, совместимость старых keys требует явного адаптера |
| GAP-08 | `OutboxHandler` at-least-once, повтор возможен после commit до ack [S08] | Item application must converge; atomic receipt является обязательной частью обработки |
| GAP-09 | `MemorySource` записывается с memory_id, event_id, evidence_ref; INSERT не содержит revision_id [S06] | Для новых изменений записывать точную связь revision ↔ source; старые связи не угадывать |
| GAP-10 | `resolve_revision_classification` отклоняет смену/очистку label [S05] | Editor label readonly; sync с другим label блокируется, не считается обычной правкой |
| GAP-11 | `MemoryConsole.tsx` есть, но в `main.tsx` нет маршрута памяти [S13–S14] | Подключить screen и SDK, а не создавать вторую Console |
| GAP-12 | В `package.json` нет общего `test`, есть typecheck/build/test:protocol [S16] | В планах вводятся новые test scripts; до этого `npm test` не использовать |
| GAP-13 | `conservative_token_count` считает ceil(UTF8 bytes / 4) [S12] | Это оценка, не доказательство строгого token cap для любого tokenizer |
| GAP-14 | Context builder может подменить пустой content строкой explanation [S12] | Новый API не должен выдавать техническое explanation за исходное знание |
| GAP-15 | MaterialIntentCommands предоставляет общую content lifecycle [S17] | Защищённый staging — новый consumer существующего механизма, не embedding-output shortcut |

## 0.3 Что не установлено

Не утверждается отсутствие любого возможного import/export кода во всём дереве: перечитаны релевантные surfaces, а не каждый файл. MW-00 обязан проверить дубли символов и pending branches до первого изменения. Не установлены времена сборки, объём RAM, текущее прохождение CI, пригодность конкретной установки к production и фактическое качество поиска.

Метод `save_memory_with_revision` содержит database CAS. Нельзя описывать его как полностью незащищённый от конкуренции. Проблема выбранного нового сценария — объединение всех его durable фактов и replay, а не отрицание существующего CAS.

## 0.4 Зависимости и стоп-условия

- Перед новым расширением утвердить связь с frozen архитектурой [S02]; пакет MW не является P05 и не закрывает P04.
- При отсутствии lawful material read/staging конкретной classification импорт блокируется. Не писать plaintext в новую временную папку/таблицу как fallback.
- Если индекс-generation не Ready, возвращать документированную недоступность через существующий runtime. Наличие source record не означает поисковую готовность.
- Прямой read/detail исторической ревизии должен проходить те же disclosure проверки, что и соответствующий retrieval. Проверять это до расширения полезной выдачи содержимого.
- Перед выполнением фиксировать новый baseline; diff относительно этого SHA должен быть рассмотрен, если main успел измениться.
