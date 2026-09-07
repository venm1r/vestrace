# Планы реализации

Все названные symbols/test targets — proposed, кроме отмеченных existing. Планы читаются вместе со specs, а не независимо от них. Номера миграций являются candidates и закрепляются в MW-00.

- [MW-00: Baseline, authority и исполнимые gates](00-preflight.md) — 2 задачи; зависимости: проект документации.
- [MW-01: Memory read и Context API](01-memory-read-context.md) — 3 задачи; зависимости: MW-00.
- [MW-02: Atomic corrections и replay](02-atomic-corrections.md) — 3 задачи; зависимости: MW-00, MW-01.
- [MW-03: Console memory workspace](03-console.md) — 3 задачи; зависимости: MW-01, MW-02.
- [MW-04: Source import и production delivery](04-import.md) — 4 задачи; зависимости: MW-02, MW-03.
- [MW-05: Sync и ручные конфликты](05-sync.md) — 3 задачи; зависимости: MW-04.
- [MW-06: Portable export/import](06-portability.md) — 3 задачи; зависимости: MW-04, MW-05.
- [MW-07: Qualification полного цикла](07-qualification.md) — 2 задачи; зависимости: MW-01, MW-02, MW-03, MW-04, MW-05, MW-06.
