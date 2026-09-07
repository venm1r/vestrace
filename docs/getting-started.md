# Начало работы: проверка среды и первый запрос

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## До запуска

Это руководство для изолированной среды разработки на baseline `07e2977`. Продукт не запускался при подготовке этой редакции. Команды ниже следует выполнять в checkout проекта, а не внутри архива документации. Для эксплуатационных данных сначала требуется проверенная процедура восстановления.

Установите toolchain, указанный в `rust-toolchain.toml` и `Cargo.toml`, Docker Compose и необходимые системные зависимости. Проверенная программа среза называет Rust 1.85/edition 2024, Node 22, PostgreSQL 17 с pgvector. Это не разрешение незаметно обновить lockfiles.

```bash
# Read-only discovery; эти команды не создают сервисы или ключи.
git rev-parse HEAD
cargo --version
node --version
docker compose version
docker compose config --quiet
```

Не публикуйте полный вывод `docker compose config`: развёрнутая конфигурация может содержать credentials. `--quiet` проверяет структуру без печати конфигурации.

## Необходимые постоянные ресурсы

Compose использует PostgreSQL data, installation fingerprint vault, provider material vault и внешний read-only bootstrap secret volume. Bootstrap-ключ НЕ генерируется образом или Compose. Пустой volume, даже правильно названный, не выполняет prerequisite. Точный layout ключа берётся из mounted-secret-store контракта конкретной сборки и подтверждается оператором.

Provisioner назначает DB-роли; отдельный migration service выполняет миграции; dev-seed создаёт локальные identity. Не подменяйте ограниченную runtime DB-роль bootstrap-администратором ради устранения ошибки. Не копируйте development-пароли в внешнюю установку.

**Стоп-условие:** пока bootstrap/layout/политика disclosure не подготовлены, полного проверенного zero-to-running пути этот документ не обещает. Задача его доведения — [F004](roadmap/p0-foundation.md#f004).

## Запуск уже подготовленной среды

Следующие команды меняют локальную среду и допустимы только после перечисленных prerequisites:

```bash
# Сохраните это имя проекта для последующих команд.
docker compose -p vestrace up --build --detach
curl --fail --silent --show-error http://127.0.0.1:8080/health/live
curl --fail --silent --show-error http://127.0.0.1:8080/health/ready
```

`live` означает доступность процесса; `ready` не квалифицирует полный embedding/import workflow. При ошибке сначала прочитайте [диагностику](operations/troubleshooting.md), а не удаляйте volumes.

## Авторизация и первый безопасный запрос

Health probes публичны в ограниченном смысле. Для `/v1/*` нужен действующий Bearer-токен с необходимыми capabilities. Его выдачу и custody выполняет разрешённая процедура установки. Не используйте `x-workspace-id`/`x-principal-id` как средство самоидентификации: middleware заменяет их результатом token authentication.

Для проверки чтения используйте доступный вам Run/list или [упражнение с Memory API](guides/memory-api-exercise.md). Примеры не содержат рабочего токена; во время выполнения не включайте shell tracing и не публикуйте terminal history с secrets.

## Остановка

```bash
# Останавливает текущую dev-среду; named volumes сохраняются.
docker compose -p vestrace down --remove-orphans
```

`--volumes` не является обычным способом устранения ошибки. Удаление БД/vault может сделать знание невосстановимым. Остановка без удаления данных всё равно не является backup.

## Дальнейшее чтение

[Console](guides/console.md) → [HTTP-правила](reference/http.md) → [Memory](reference/memory.md) → [состояние возможностей](status.md). Будущий folder import описан в MW, а не представлен работающей CLI-командой.

---
**Основание:** [R03: crates/vestrace-cli/src/main.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-cli/src/main.rs), [R04: docker-compose.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml), [R05: docs/getting-started.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/getting-started.md), [R01: crates/vestrace-http/src/auth.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md).

[Карта документации](README.md) · [Состояние и ограничения](status.md) · [Реестр источников](maintenance/sources.md)
