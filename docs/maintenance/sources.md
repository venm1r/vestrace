# Источники и пределы проверки

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Реестр источников, не product qualification.

## Baseline

Репозиторий — `venm1r/vestrace`; согласованный пользователем baseline — `07e2977a20b05c5b16953a206a6d68bdbff3a052`. Его Git parent — `6f6102536e9a535b7086db14573bf45fe750ad71`; comparison показывает один docs integration commit. Поэтому прежние выбранные code observations сохранены с исходным pin, а документационная integration отражена отдельно.

Изучены выбранные source files и текущие инструкции; весь runtime не запускался и весь репозиторий построчно не аудировался. S-ссылки наследуют явно указанный scope анализа MW, R-ссылки — повторно прочитанные файлы/дополнения. В частности, S02 относится к прежней версии normative index и не подменяется новым hash.

## Реестр

| ID | Источник | Область использования |
| --- | --- | --- |
| **S01** | [docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md) | 14B–14D и worker --once; 14D принят до ResultPrepared, 14E — предложение. |
| **S02** | [docs/specs/README.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/specs/README.md) | Иерархия нормативных документов; отсутствие автоматического расширения roadmap. |
| **S03** | [crates/vestrace-application/src/memory/mod.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/mod.rs) | Действующий MemoryUseCases, без list/history read-модели. |
| **S04** | [crates/vestrace-application/src/memory/ports.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/ports.rs) | MemoryRepository и атомарная запись memory/revision/source/search. |
| **S05** | [crates/vestrace-application/src/memory/services.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs) | Outbox и idempotency сохраняются отдельно; fingerprint исключает случайные ID; label transition запрещён. |
| **S06** | [crates/vestrace-infrastructure/src/postgres/memory_repository.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-infrastructure/src/postgres/memory_repository.rs) | begin_scoped, CAS памяти, запись ревизии и источника; схема memory_sources без revision_id в INSERT. |
| **S07** | [crates/vestrace-application/src/governed_mutation.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/governed_mutation.rs) | GovernedMutationRepository::commit_in и GovernedMutationApply. |
| **S08** | [crates/vestrace-application/src/outbox.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs) | At-least-once; обработчик обязан быть идемпотентным; 5 попыток, backoff, dead-letter. |
| **S09** | [crates/vestrace-application/src/idempotency.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/idempotency.rs) | IdempotencyRepository::save_in; запись содержит workspace и срок хранения. |
| **S10** | [crates/vestrace-http/src/api/memory.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs) | MemoryResponse — метаданные, без content и номера активной ревизии. |
| **S11** | [crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs) | ContextPackDto — сводка без секций; temporal и intent параметры уже есть. |
| **S12** | [crates/vestrace-application/src/retrieval/context_builder.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/retrieval/context_builder.rs) | ContextItem.rendered_text и provenance_refs; utf8-bytes/4 как оценка токенов; fallback на explanation. |
| **S13** | [apps/console/src/memory/MemoryConsole.tsx](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/memory/MemoryConsole.tsx) | Компонент памяти существует; данные передаются props, без собственного редактирования. |
| **S14** | [apps/console/src/main.tsx](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/main.tsx) | Memory routes отсутствуют в таблице Routes. |
| **S15** | [apps/console/src/sdk/client.ts](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk/client.ts) | Общий ApiRequestError и текущие client DTO; расширять, не создавать параллельный auth-клиент. |
| **S16** | [apps/console/package.json](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/package.json) | React, TypeScript, Vite; есть test:protocol, нет общего npm test. |
| **S17** | [crates/vestrace-application/src/material/commands.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/material/commands.rs) | MaterialIntentCommands: reserve/prepare_content/bind/finalize_bound; восстановление не выдумывает утраченный plaintext. |
| **S18** | [apps/console/src/routes](https://github.com/venm1r/vestrace/tree/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/routes) | Полный список routes в поддереве, truncated=false; MemoryPage отсутствует. |
| **S19** | [apps/console/src/sdk](https://github.com/venm1r/vestrace/tree/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk) | client.ts, agUiClient.ts, useApiResource.ts. |
| **R01** | [crates/vestrace-http/src/auth.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs) | Bearer identity; read lines 85–160 |
| **R02** | [crates/vestrace-http/src/route_inventory.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/route_inventory.rs) | Route inventory; read lines 1–250; not proof of every handler |
| **R03** | [crates/vestrace-cli/src/main.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-cli/src/main.rs) | Clap command surface; read lines 1–320 |
| **R04** | [docker-compose.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml) | Read lines 1–110 and 340–495; bootstrap volumes and browser proxy |
| **R05** | [docs/getting-started.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/getting-started.md) | Original guide, complete read; contains stale claims |
| **R06** | [docs/domain-model.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/domain-model.md) | Original domain entry, complete read |
| **R07** | [docs/database-schema.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/database-schema.md) | Original schema entry, historical snapshot |
| **R08** | [docs/security-and-rls.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/security-and-rls.md) | Original security entry, historical snapshot |
| **R09** | [docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md) | Normative contract from preceding pinned review; target, not implementation |
| **R10** | [docs/adr/0001-memory-first-persistent-cognition.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/adr/0001-memory-first-persistent-cognition.md) | Accepted product boundary from preceding pinned review |
| **R11** | [docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md) | Fixed P01–P12 program from preceding pinned review |
| **R12** | [.github/workflows/ci.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/.github/workflows/ci.yml) | CI definition from preceding pinned review, no new CI execution |
| **R13** | [crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs) | Exact revision hydration from preceding pinned review |
| **R14** | [scripts/p04-scope.mjs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/scripts/p04-scope.mjs) | Protected authorities from preceding pinned review |
| **R15** | [apps/console/vite.config.ts](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/vite.config.ts) | Read at 07e2977; Vite identity injection is not Bearer authentication. |
| **R16** | [apps/console/nginx.conf.template](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/nginx.conf.template) | Read at 07e2977; nginx attaches deployment token, loopback trust boundary. |
| **R17** | [crates/vestrace-mcp/src/server.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-mcp/src/server.rs) | Read get_memory/search on unchanged code ancestor; metadata-only result. |
| **R18** | [docs/implementation/memory-workspace/README.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/implementation/memory-workspace/README.md) | Integrated MW tree verified against Git tree 1f69acad; existing proposal preserved. |

Машиночитаемая версия — [sources.json](sources.json). Git blob SHA позволяет проверить байты конкретного источника; он не доказывает успешное исполнение. Некоторые entries являются directory metadata или документами требований, что указано в их исходных полях.

## Внешние справочники

Предыдущий пакет содержит первичные ссылки Cargo/PostgreSQL. Они используются как справочные основания соответствующих contracts, а не как сведения о текущей версии Vestrace. Свежий сравнительный аудит конкурентов и актуализация цен/рыночных долей в это задание не входят.

## Входные документы

См. [input reconciliation](input-reconciliation.md). Пароли, токены, временные GitHub download URLs и приватные данные пользователей не включаются в реестр.

---
[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](sources.md)
