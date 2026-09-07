# Соответствие приоритетов существующим программам

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Правило совместимости

У новой дорожной карты нет права отменять старое обязательство, менять число frozen пакетов или присваивать им PASS. Исторический 36-PR transition сохраняется для traceability; его номера не смешиваются с нынешними P01–P12 и MW-00–MW-07.

| Frozen package | Предмет | Приоритетная связь | Действие |
| --- | --- | --- | --- |
| P01 | Baseline/protocol locks | P0/F006 | Переиспользовать принятые locks и проверять актуальность |
| P02 | Security/material/atomic authority | P0/F002/F003/F008 | Не дублировать; новые consumers проходят те же gates |
| P03 | Connections/models/provider/effects | P0 foundations; P3/F301 | Не возвращать config-only executor или обход data policy |
| P04 | Embedding/corpus/generation | P0/F001; P2/F207 | Завершить оставшийся lifecycle; 14D не равен full closure |
| P05 | Restore/installation/roles/G0 | P0/F004/F005 | Обязательная эксплуатационная граница |
| P06 | Реальные config/agents/Runs | P3/F301 | Остаётся обязательным для полного v1.0 |
| P07 | Interaction/artifact/retention | P3/F302 | Не подменять новым importer runtime |
| P08 | Полный AG-UI | P3/F303 | Принимается pinned официальным клиентом |
| P09 | A2A server | P3/F304 | Сначала server compatibility |
| P10 | Outbound A2A | P3/F304 | Затем external step/ambiguity contract |
| P11 | Остальные menu workflows | P3/F305 | Реальные действия, без enabled stubs |
| P12 | Exact-environment release | P3/F306 | Только fresh evidence и owner release decision |

## Memory Workspace

| MW package | Приоритетные инициативы | Статус |
| --- | --- | --- |
| MW-00 | Baseline/scope для всех P1 | Proposed plan, owner review required |
| MW-01 | F101/F102/F108 | Полезное чтение и контекст |
| MW-02 | F003/F103 | Атомарное исправление и повтор |
| MW-03 | F104 | Console библиотека/редактор |
| MW-04 | F105 + F007/F008 | Источники, preview, worker apply |
| MW-05 | F106 | Sync conflict, cancel, missing |
| MW-06 | F107 | Переносимость |
| MW-07 | F109/F204 + F005/F006 | Feature qualification/upgrade, не P12 |

## Решение о размещении в релизе

Рекомендуемый default: сохранять MW отдельным именованным milestone до принятия owner amendment. Более ранняя bounded alpha не выдаётся за полный v1.0. Если владелец включает MW в v1, уточняются P12 shipping manifest, зависимости и scope; фраза «добавили docs» этим amendment не является.

Новые P2/P4 capabilities получают отдельные подробные specs после проверки предпосылок. Они не становятся P13+ автоматически. Команда может согласованно выбрать отдельную ветвь M4, не блокируя обязательный P06–P12 путь ради ещё одной сводки памяти.

## Неослабляемые пересечения

Shared memory writer, current content access, material read/staging, idempotency namespace, lock order и generation readiness принадлежат существующим властным границам. Если эти пути меняются одновременно, сначала принимается shared contract, затем потребители. Простое объединение Git branches не разрешает semantic conflict.

---
**Основание:** [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R18: docs/implementation/memory-workspace/README.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/implementation/memory-workspace/README.md), [S07: crates/vestrace-application/src/governed_mutation.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/governed_mutation.rs), [S01: docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
