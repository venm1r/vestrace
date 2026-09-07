# Сопровождение документации

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Редакционные правила новой документации.

## Единые источники

Current status опирается на source observation с exact pin. Нормативные specs/ADR сохраняют authority. Roadmap feature-register.json является источником приоритетных карточек. MW package хранит свои требования, схемы, examples и plans один раз. Монолитный Markdown и ZIP — производные поставки.

## Когда обновлять

При изменении API/controller/route inventory обновить reference и examples. При замыкании production path добавить новый факт со ссылкой на observed evidence; не исправлять старый verdict как будто он описывал новую версию. При изменении roadmap dependencies проверить acyclic graph и release relationship. При изменении MW — пересчитать его manifest по собственной процедуре и сохранить исторический отчёт, а не подделывать прежнюю проверку.

## Статусы

Использовать SOURCE_PRESENT, PARTIAL_SURFACE, RECORDED_PARTIAL_ACCEPTANCE, DESIGN_ONLY или явно NOT_AUDITED. Для выполненной проверки нужны команда, environment, input digest и результат. Маркер NOT_RUN_HERE не исправлять на PASS при простом review текста.

## Проверка

В этой поставке имеется отдельный documentation validator: links/anchors, JSON, sources/dependencies, corpus refs, безопасный scope patch и сохранённый MW tree. Он не проверяет product behavior и не присваивает qualification. Внешние URL live-check автоматически не выполняется.

### Повторить проверку этой поставки

Проверяйте распакованную поставку в отдельном каталоге. Python 3.10+; нужны `markdown-it-py` и `jsonschema`. Зависимости устанавливаются только в выбранную вами среду. Из корня распакованной поставки:

```bash
python docs/maintenance/validate_documentation.py .
```

Валидатор не обращается к сети и не изменяет файлы. Bash-примеры проверяются только через `bash -n`, не выполняются. `payload-manifest.json` фиксирует байты поставки; после сознательного редактирования несовпадение ожидаемо и требует новой проверки/манифеста. Это не тест runtime Vestrace.

## Изменения и интеграция

Архив — overlay к repository 07e2977. Не удалять существующий `docs/`, не переименовывать исторические authority files. Предпочтительно применять проверенный patch в отдельной рабочей ветке с review. Перед применением к иному commit провести новый delta.

Новые docs-only changes не дают разрешение изменять code/migrations/protocol locks. Для выполнения feature-плана нужен отдельный accepted scope и проверка dependencies.

---
[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](sources.md)
