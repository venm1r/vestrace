# Источники и границы проверки

Базовый commit: `6f6102536e9a535b7086db14573bf45fe750ad71`. Дата чтения: 2026-09-07.

Ссылки S01–S19 относятся к одному зафиксированному commit. Хеш Git blob относится ко всему файлу; ranges в JSON показывают реально прочитанные участки. Совпадение хеша не означает выполнения тестов.

Ниже приведены наблюдения из кода, а не результаты сборки. Документ не является полным аудитом репозитория. Планируемые файлы отделены от существующих в `file-plan.json`.

## S01
[docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md)

14B–14D и worker --once; 14D принят до ResultPrepared, 14E — предложение.

## S02
[docs/specs/README.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/specs/README.md)

Иерархия нормативных документов; отсутствие автоматического расширения roadmap.

## S03
[crates/vestrace-application/src/memory/mod.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/mod.rs)

Действующий MemoryUseCases, без list/history read-модели.

## S04
[crates/vestrace-application/src/memory/ports.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/ports.rs)

MemoryRepository и атомарная запись memory/revision/source/search.

## S05
[crates/vestrace-application/src/memory/services.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs)

Outbox и idempotency сохраняются отдельно; fingerprint исключает случайные ID; label transition запрещён.

## S06
[crates/vestrace-infrastructure/src/postgres/memory_repository.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-infrastructure/src/postgres/memory_repository.rs)

begin_scoped, CAS памяти, запись ревизии и источника; схема memory_sources без revision_id в INSERT.

## S07
[crates/vestrace-application/src/governed_mutation.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/governed_mutation.rs)

GovernedMutationRepository::commit_in и GovernedMutationApply.

## S08
[crates/vestrace-application/src/outbox.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs)

At-least-once; обработчик обязан быть идемпотентным; 5 попыток, backoff, dead-letter.

## S09
[crates/vestrace-application/src/idempotency.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/idempotency.rs)

IdempotencyRepository::save_in; запись содержит workspace и срок хранения.

## S10
[crates/vestrace-http/src/api/memory.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs)

MemoryResponse — метаданные, без content и номера активной ревизии.

## S11
[crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs)

ContextPackDto — сводка без секций; temporal и intent параметры уже есть.

## S12
[crates/vestrace-application/src/retrieval/context_builder.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/retrieval/context_builder.rs)

ContextItem.rendered_text и provenance_refs; utf8-bytes/4 как оценка токенов; fallback на explanation.

## S13
[apps/console/src/memory/MemoryConsole.tsx](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/memory/MemoryConsole.tsx)

Компонент памяти существует; данные передаются props, без собственного редактирования.

## S14
[apps/console/src/main.tsx](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/main.tsx)

Memory routes отсутствуют в таблице Routes.

## S15
[apps/console/src/sdk/client.ts](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk/client.ts)

Общий ApiRequestError и текущие client DTO; расширять, не создавать параллельный auth-клиент.

## S16
[apps/console/package.json](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/package.json)

React, TypeScript, Vite; есть test:protocol, нет общего npm test.

## S17
[crates/vestrace-application/src/material/commands.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/material/commands.rs)

MaterialIntentCommands: reserve/prepare_content/bind/finalize_bound; восстановление не выдумывает утраченный plaintext.

## S18
[apps/console/src/routes](https://github.com/venm1r/vestrace/tree/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/routes)

Полный список routes в поддереве, truncated=false; MemoryPage отсутствует.

## S19
[apps/console/src/sdk](https://github.com/venm1r/vestrace/tree/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk)

client.ts, agUiClient.ts, useApiResource.ts.

## E01
[https://doc.rust-lang.org/cargo/commands/cargo-test.html](https://doc.rust-lang.org/cargo/commands/cargo-test.html)

--all-targets не включает --doc; doctests запускаются отдельно.

## E02
[https://www.postgresql.org/docs/17/ddl-rowsecurity.html](https://www.postgresql.org/docs/17/ddl-rowsecurity.html)

Superuser/BYPASSRLS обходят RLS; FORCE не отменяет superuser bypass.
