# Тестирование: уровни, чувствительность и ресурсы

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Уровни

| Уровень | Что доказывает | Что не заменяет |
| --- | --- | --- |
| Domain/unit | Локальную логику и допустимые transitions | Runtime wiring, БД, права |
| Contract/schema | Формат и compatibility | Реальное выполнение |
| PostgreSQL | Constraints, UoW, grants, races на данной БД | Provider/browser interop |
| Composition/E2E | Штатные entrypoints и полный путь | Другие непроверенные топологии |
| Fault/restart | Выбранные границы смерти процесса | Произвольный power-loss или все сбои |
| Product quality | Полезность контекста на corpus | Security/qualification |
| Release | Замыкание всех declared gates на target | Стабильность другого target |

## Команды текущего toolchain

Ниже — команды для выполнения в кодовом checkout с подготовленной test DB, не результаты этого документационного задания:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked --no-fail-fast
cargo test --workspace --doc --all-features --locked
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
```

Doctests выделены явно: `--all-targets` не является их заменой. В проверенном CI это отдельный рекомендуемый delta, а не якобы уже работающий шаг. Точные test targets MW становятся запускаемыми после их реализации.

## RED и mutation

Сначала получить корректный fixture setup, затем поведенческое падение утверждения. Отсутствующий DATABASE_URL, несуществующий test target или compilation error — не доказательство чувствительности business test. Для критичного guard временное ослабление должно сделать неизменённую проверку красной; после восстановления exact bytes — снова зелёной. Записывать оба наблюдения.

## Runtime роль

Отдельная admin identity может подготовить одноразовую БД и grants по deployment rules. Runtime часть проверяется ограниченным пользователем. Иначе успешная запись может скрывать production permission gap, а отсутствие отказа — оказаться полномочиями администратора.

## Ресурсная стоимость

Измерять холодную сборку, цикл правки, peak RAM/disk и размеры test binaries. Не запускать два Cargo build одновременно по умолчанию. Изменение debug/incremental profiles и объединение test targets проводить как измеряемый эксперимент, не отключая негативные классы проверок.

## Документационные проверки отдельно

В этом комплекте валидируются ссылки, JSON, граф зависимостей, scope diff и сохранность MW. Такие проверки не дают evidence о Rust/SQL/browser поведении. Их результаты хранятся в maintenance, а не в production qualification bundle.

---
**Основание:** [R12: .github/workflows/ci.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/.github/workflows/ci.yml), [S16: apps/console/package.json](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/package.json), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
