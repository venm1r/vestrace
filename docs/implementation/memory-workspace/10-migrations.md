# 10. Миграции, совместимость и хранение

## 10.1 Существующая граница

Baseline evidence содержит 0192–0194 и proposal 0195. Программа MW не меняет их checksums и не занимает 0195. Candidate номера ниже должны быть подтверждены по live tree при MW-00; при коллизии номер меняется до любой DB application, вместе с file-plan и tests. Это не permission редактировать уже применённую миграцию.

| Logical migration | Candidate path | Содержание |
|---|---|---|
| MW-M01 | `migrations/0196_memory_workspace_receipts.sql` | MW-01 создаёт revision links, epoch triggers и receipt schema; MW-02 использует их, не меняя применённый SQL |
| MW-M02 | `migrations/0197_source_import_authority.sql` | collections/sources/revisions, initial memory_source_bindings, operation/items, ordinary payload owner bindings |
| MW-M03 | `migrations/0198_source_sync_conflicts.sql` | conflict triple, resolution authority and transitions of the already-existing manual-override binding |
| MW-M04 | `migrations/0199_memory_export_authority.sql` | pinned export selection, material result binding, portable import mappings |

Названия source_revisions/links/conflicts проверяются на существующие агрегаты. Если уже есть подходящая authority, расширяется она, а не создаётся одноимённая параллельная. Предложенные таблицы из 02 — logical contract, не лицензия обходить общий InstallationMutationPermit.

## 10.2 M01 и общий writer

M01 применяется в MW-01. Epoch изменяется транзакционными DB triggers при изменении канонических memory/revision/link rows; trigger покрывает и legacy writers. MW-00 проверяет весь набор допустимых write путей. MW-02 получает готовую schema и не правит M01. Его writer не увеличивает epoch второй раз: увеличение принадлежит trigger. При последующем добавлении источников M02 добавляет соответствующие triggers новой forward migration. Все epoch locks располагаются в одном согласованном месте transaction order; два оператора не берут сначала epoch, затем memory в обратном порядке.

Добавить FK-bound revision-source provenance для новых revisions. Старые связи не backfill-ить guessed timestamps. Epoch rows создаются для текущих workspaces через установленный privileged migration/provisioning путь; обычный runtime не перечисляет все workspaces для «удобства». Наличие таблицы проверяется schema gate до запуска новых routes.

Ввод correction receipts не означает второй idempotency service. Использовать нынешнюю `idempotency_keys` authority и scoped namespace; typed correction receipt может быть row-проекцией committed result, но не независимым winner. Если текущая таблица требует расширения scope uniqueness, миграция обязана обеспечить однозначную совместимость старых ключей, не пересопоставляя их молча другому principal.

Legacy completed receipts остаются историческими. Для неистёкшего legacy ключа preserve existing exact replay; новый scoped key не должен неожиданно «заново» применить повтор старой операции. Реализация переводит legacy write адаптеры на единый atomic participant, однако не меняет их request/response shapes. Истёкшая receipt не воскрешается из content hash. Для new correction удержание receipt определяется operation lifecycle, без чувствительного текста в response_payload.

## 10.3 M02–M03 и reference integrity

Новые scoped tables имеют composite workspace FK, closed states, unique `(collection,external_id)`, unique source revision ordinal и at most one binding memory/source. Source payload material доступен только из того же workspace и approved ordinary content owner. SourceRevision не ссылается на временный file path или чужой embedding output.

Preview сохраняет bases и exact payload identity. Operation/input mapping не меняется после PreviewReady. Apply selection receipt и переход в applying атомарны с outbox enqueue. Item receipt и source/memory changes коммитятся вместе. Source revision и active source locator могут продвинуться при конфликте только с соответствующим immutable conflict record; effective memory остаётся M до принятого resolution.

Все mutating SQL entrypoints требуют scoped owner/context. RLS не заменяет business actor permission, как privilege не заменяет invariants. Новый runtime должен получать EXECUTE только на конкретные signatures, а не на все функции schema. Ownership и grant allowlists в `docker/postgres/init-runtime-role.sh` обновляются согласованно; test derives catalog and explicitly asserts new required names, а не только минимальное количество rows.

## 10.4 M04 и package mappings

Foreign IDs хранятся в namespace `(workspace,import_operation,package_id,foreign_kind,foreign_id)`. Нельзя установить foreign UUID как доверенный local memory ID. Ключ maps local revision IDs до linking pass; ссылки, не прошедшие проверку, оставляют explicit partial state. Unknown future package schema отказывается до canonical mutations.

Export pin/selection выбираются under consistent read scope. Result material связан с export operation и каждой сохранённой dependency; generic download artifact URL не допускается. Право на download проверяется сейчас, даже когда подготовка происходила под более широкими grants.

## 10.5 Retention compatibility

New revision/source links участвуют в действующем purge/erasure inventory. Добавление FK не должно превратить lawful memory purge в необъяснимый foreign-key error, а CASCADE не должен удалить shared source других memories. Runtime tests выполняют existing purge command на disposable данных с новыми bindings, staged preview и export, проверяют каждый retained/erased outcome.

Memory-source relationships и receipt safe identity могут пережить erasure как tombstone, если это разрешено принятой retention policy; content и запрещённые locators не копируются в tombstone. Архив пользователя не может быть отозван — это не свойство server cleanup.

## 10.6 Upgrade/fallback

Проверить fresh database и **upgrade populated baseline**: старая memory r1/r2, existing effects, outbox, material state и old idempotency receipts. После upgrade прежние routes и P02/P03/P04 сценарии остаются корректны; новая библиотека видит старые данные с honest provenance status.

Rollback продукта — остановить новые admissions/дождаться или отменить операции, проверить совместимость старой версии с новой схемой и данными. Автоматический down-migration или удаление таблиц не предлагается. Backup/restore полной установки не заменяется portable JSON. Новый content owner требует отдельной проверки действующего recovery/restore механизма, прежде чем говорить о поддерживаемом обновлении.
