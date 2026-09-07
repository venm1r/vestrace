# 08. Программа реализации Memory Workspace

**Baseline:** `6f6102536e9a535b7086db14573bf45fe750ad71`. **Статус:** proposed, не изменение P01–P12 и не авторизация записи в защищённые paths.

## Порядок и границы

| Пакет | Результат | Предшественники | Порог завершения |
|---|---|---|---|
| MW-00 | Принятый delta/scope и карта действующих зависимостей | этот проект | Fresh tree, точные file/authority границы, отсутствие конфликтов с P04 |
| MW-01 | Read schema/epoch, detail/history/browse и полезный context через единый read path | MW-00 | Runtime content-disclosure tests; context gated отдельно от чтения |
| MW-02 | Единый atomic writer, correction/restore и replay | MW-00, MW-01 | One UoW, CAS, exact replay, no legacy bypass |
| MW-03 | Подключённая библиотека/редактор Console | MW-01, MW-02 | Реальный browser → HTTP → DB edit/history/conflict |
| MW-04 | Source import: коллекции, staging, preview, scanner, handler | MW-02, MW-03 | Exact-byte preview, lawful material owner, worker apply |
| MW-05 | Повторная sync, B/I/M conflicts, отмена и Missing | MW-04 | Ручная правка сохраняется; две стороны race доказаны |
| MW-06 | Export/portable import и UI/CLI | MW-04, MW-05 | Перенос без authority laundering, защищённый download |
| MW-07 | Upgrade, негативные проверки и полная приёмка | MW-01–MW-06 | Работает один целый сценарий и зафиксированы ограничения |

Пакеты идут в таком порядке, чтобы каждый принимался отдельно. Разработка read частей MW-01 может идти до окончания 14E, но ContextPack gate не объявляется принятым без настоящей доступной retrieval generation. MW-04 не запускает дополнительные model calls и не чинит P04 обходом; действующие embedding outbox topics после memory mutation сохраняют свои обязательства и blockers. Новую source операцию нельзя объявить indexed на основании одного commit памяти.

## Планирование на актуальном дереве

[Карта файлов](file-plan.json) различает прочитанные существующие files, известные references для повторного чтения и proposed новые paths. Это не универсальная allowlist на все пакеты. Перед пакетом builder фиксирует его own preflight, exact paths и protected authorities. Новый файл или dependency требует отдельного scoped amendment; старые P04 lists не редактируются этой программой.

Candidate migrations 0196–0199 — имена для оценки diff, не зарезервированные номера. 0195 назван в P04/14E. Если на момент MW-00 any number занят, переименовать все связанные ссылки одним документационным amendment до первого SQL write.

## Исполнение и evidence

Builder реализует один тестируемый task; reviewer проверяет контракт, а не только diff. Этапы: спецификация → поведенческий RED → минимальная реализация → GREEN → negative/mutation приёмка → независимый review → запись evidence. Review, заявленный builder, не является независимым review. Полный прогон не заменяется grep выбранных успешных строк.

Планы содержат конкретные proposed tests и команды. Они становятся запускаемыми **после** создания названного test target/fixture; до этого ошибка «target not found» не считается содержательным RED. Test fixture сначала поднимает isolated supported runtime и создаёт данные через production entrypoints, затем assertion ломается на отсутствующем поведении. Database migrations/admin setup — единственное допустимое административное приготовление; completion rows, source data и receipts не сеются напрямую для E2E.

## Общие проверочные команды

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked --no-fail-fast
cargo test --workspace --doc --all-features --locked
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
```

Это рекомендации для существующего toolchain и доступной test-среды, не наблюдённый результат. PostgreSQL URLs/credentials подаются через безопасный setup, не копируются в evidence. Два Cargo процесса одновременно не запускать по умолчанию. `--all-targets` не заменяет doctests [E01]. Playwright scripts и tests ниже добавляются MW-03, их нет в baseline:

```bash
npm --prefix apps/console run test:memory
npm --prefix apps/console run test:e2e:memory
```

## Документы для agentic handoff

Каждый [план пакета](plans/README.md) ссылается на общие specs. Вся информация для тестового сценария находится в [приёмке](09-acceptance.md) и [JSON fixtures](examples/catalog.json). Proposed interfaces описаны в соответствующем task; по совпадению имени в планах нельзя заключать, что такой символ уже есть в коде.

Gate перед commit/merge: exact scoped diff, no secret payloads, confirmed failure paths, full relevant suites, recorded unavailable dependencies. Нет разрешения push/deploy из этого документа. Продуктовый release требует отдельного решения с evidence по конкретному supported environment.
