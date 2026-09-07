# Нормативные основания и поясняющие руководства

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Навигация по сохранённым нормативным документам; не новый нормативный контракт.

Нормативная иерархия не изменена: Architecture Contract → явно уточняющий более новый Accepted ADR → специализированные specs/invariants. Новые руководства и roadmap поясняют и планируют, но не переписывают эту authority. Слова MUST в Proposed MW означают проект будущего контракта, а не уже принятое изменение baseline.

## Сохранённые документы

- [Architecture Contract](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md)
- [Domain Model](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-domain-model-v0.2.md)
- [Invariants](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-normative-invariants-v0.2.md)
- [Trust / Authority](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-trust-authority-model-v0.2.md)
- [Data / Temporal](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-data-temporal-model-v0.2.md)
- [Execution / Effects](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-execution-external-effects-contract-v0.2.md)
- [Health / Repair](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-health-repair-incident-contract-v0.2.md)
- [Crypto / Data Governance](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-crypto-data-governance-contract-v0.2.md)
- [Qualification](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-qualification-conformance-spec-v0.2.md)
- [Frozen version roadmap](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-version-roadmap-v0.2-to-v1.0.md)

Оригиналы по прежним путям сохраняются в репозитории. В этом архиве-переработке они не продублированы; pinned ссылки открывают ровно исходный срез. Изменения только оглавления не меняют тела спецификаций, requirement IDs или квалификацию.

## Accepted расширения и новые proposals

Brain–Face–Organ остаётся принятым system-level extension с собственными границами. [Обзор архитектуры](../architecture.md) поясняет его, но не объявляет дополнительный runtime реализованным.

[Memory Workspace](../implementation/memory-workspace/README.md) — отдельный интегрированный Proposed design. [Новая roadmap](../roadmap/README.md) — приоритизация дальнейших решений. Ни один из них не заменяет утверждённые release gates без явного amendment.

## Реализация и история

Для source reality читать [status](../status.md), а для старых результатов — [history](../history/README.md). Нарушение кода относительно нормы записывается как gap; сам факт, что код так работает, не отменяет норму.

---
**Основание:** [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R10: docs/adr/0001-memory-first-persistent-cognition.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/adr/0001-memory-first-persistent-cognition.md), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
