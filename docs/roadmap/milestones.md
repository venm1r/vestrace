# Milestones: вход, результат и выход

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предложенная milestone-система; даты и acceptance не назначены.

## M0 — baseline и порядок исполнения

**Вход:** commit 07e2977 и доступные архивы. **Работа:** зафиксировать code/doc distinction, reread изменяемых symbols, dirty/scope, prerequisites и schedule branch. **Выход:** owner принял конкретный первый scope, существующие gates не перезаписаны, новые проверки названы и воспроизводимы. Документационная поставка выполняет только часть подготовки, не operator approval кода.

## M1 — foundation

**Вход:** принятые предшественники P04 и P05. **Работа:** delivery binding/publication, остальные P04 query/rebuild paths и worker wiring; setup/roles/freeze/restore/diagnostics. **Выход:** реальное terminal состояние достигается без ручного SQL и сохраняется через crash; full P04/G0 принимаются только по их evidence. Отдельные готовые read-only задачи могут проектироваться раньше.

## M2 — управляемая память

**Вход:** MW-00, lawful current read и базовая безопасность. **Работа:** read/history/browse, context output, единая atomic mutation и Console. **Выход:** пользователь читает exact revision, исправляет, получает conflict при гонке и видит результат после reload. Context readiness квалифицируется независимо от успеха detail.

## M3 — пользовательские источники

**Вход:** MW-02/03 и material/scheduling dependencies. **Работа:** bounded source import, preview/apply, three-way sync, missing/cancel, portability и upgrade. **Выход:** один полный folder → memory → editorial change → updated folder → conflict resolution → permitted export сценарий проходит через API/worker/browser. Отсутствие LLM в importer не является недостатком этого milestone.

## M4 — полезность на реальных задачах

**Вход:** работающий P1 сценарий и первые наблюдения. **Работа:** выбрать temporal/conflicts/inspector/integration по проблеме; затем derived summaries при доказанной пользе. **Выход:** versioned corpus показывает конкретное улучшение без safety regression. Сравниваются одинаковые данные, модель, разрешения и budget. Не нужно выполнить все семь инициатив, чтобы начать пилот или двигать frozen v1 track.

## M5 — полный релизный контракт

**Вход:** P05 и последовательность P06→P07→P08/P09→P10→P11; readiness включённых MW отдельно. **Выход:** P12 на точном окружении, подписанная/принятая release policy по существующему контракту, обновление/restore и известные ограничения. Разработческий PASS или star count не заменяет gate.

## M6 — расширение

**Вход:** пользователи возвращаются к задаче, а её эксплуатация измерена. **Работа:** только выбранные команды/hosting/SDK/formats/harness directions. **Выход:** новый отдельный scope, владелец поддержки и проверка threat/operations model. Не обещать cluster/federation на основании local single-workspace результата.

## Когда остановить расширение

При нарушении доступа, неразрешённой потере данных, невоспроизводимом migration upgrade или новом повторе внешнего эффекта приоритет возвращается к P0 затронутого пути. При плохом onboarding — к P1. При отсутствии полезного повторного применения — к проверке сценария, а не автоматически к P4.

---
**Основание:** [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R18: docs/implementation/memory-workspace/README.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/implementation/memory-workspace/README.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
