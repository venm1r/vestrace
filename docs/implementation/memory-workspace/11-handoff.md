# 11. Передача исполнителю и решения о начале

## Статус поставки

Это проект implementation documentation на commit `6f6102536e9a535b7086db14573bf45fe750ad71`. Автор этой поставки прочитал выбранные sources; **не запускал Vestrace, PostgreSQL, Rust suites, browser E2E, migration upgrade или actual import/export**. Документы/JSON-примеры могут быть проверены отдельно, и только этот результат находится в verification/report.md.

Каталог интегрирован в документационные входы репозитория как proposed feature design; расположение и иерархия описаны в [12-integration.md](12-integration.md). Документационный commit/PR не реализует продукт и не принимает новые требования v1.0. Первым рассматривается MW-00; при занятом scope/P04 baseline выполнение откладывается или документы явно пересматриваются.

## Зафиксированные проектные решения для принятия

Предлагаются 12 решений MW-D01–MW-D12 из 01-design: аддитивный API, один writer, immutable source vs manual revision, bounded whole-document importer, exact preview, atomic item и partial batch, same outbox, Missing не Delete, ordinary material staging, portable export без authority, feature-scoped acceptance.

Принятие документации должно отдельно определить её место в release program: новый самостоятельный milestone или явно согласованное amendment существующего. По умолчанию этот пакет **не** включён молча в frozen P01–P12 и не может ослабить их gates.

## Порядок действий исполнителя

1. Получить fresh HEAD и проверить изменения относительно `6f6102536e9a535b7086db14573bf45fe750ad71`. Прочитать 00-baseline, source-manifest, состояние 14E и последующих работ.
2. Для MW-00 заполнить новый preflight с exact files/refs, выбрать доступный supported local test environment и проверить отсутствие одноимённых modules/tables/routes.
3. Принять/уточнить contract gaps: ordinary material source owner/read, memory content policy, idempotency namespace compatibility, shared lock order и qualified tokenizer mode. Это конкретные dependency checks, не право заменить их mocks.
4. После разрешения scope перейти к MW-01/02 по планам. Тесты сначала отличаются на реальном поведении; setup failures записываются отдельно.
5. После каждого task записывать evidence и known limitations. Independent reviewer проверяет конечный diff и соответствие требованиям; advisory plan review не засчитывается final code review.
6. Не публиковать «feature complete», пока выбранный end-to-end gate не выполнен, включая final rendering, source synchronisation и сохранность manual edits.

## Непереговорные отказы

Нельзя добавлять новый vault, parallel event store или scheduler; отключать generation/label/role checks ради demonstration; сеять success receipts как администратор; silently overwrite human edits; превращать imported trust/actor в local authority; использовать прямой SQL клиента; называть readonly byte-limit режим доказанным hard-token ContextPack; переносить gates между разными commits без revalidation.

## Формат evidence на один task

```text
Task / requirements / exact input baseline
Changed files and scope authorization
Environment and role identities (without secrets)
RED command / actual assertion / exit
Implementation revision
GREEN command / counts / exit
Mutation / restore digest / GREEN
Independent review status and reviewed revision
Remaining blockers; what was not run
```

Шаблон предназначен для будущего заполнения фактическими наблюдениями, не содержит фиктивного PASS. Generated UUIDs примеров — синтетические. Предложенный порт/тест не является существующим symbol. OpenAPI proposed subset не заменяет `vestrace schema http`.

## Definition of delivered documentation

Документы считаются согласованным проектом, когда ссылки/schema/examples/traceability не противоречат друг другу и scope/dependencies обозначены. **Готовность реализации** проверяется отдельным MW-07 на фактической установленной системе. Регистрация этого пакета в документации означает только первую границу.
