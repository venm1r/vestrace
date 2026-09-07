# Console: доступ, состояние и выбранное развитие

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Два режима — разные предпосылки

Контейнерная Console обслуживается nginx. Прокси добавляет `Authorization` из runtime environment и не отправляет этот token в JS bundle. Доступ к порту поэтому фактически даёт возможности configured token. Порт по умолчанию привязан к loopback; выставление наружу требует другой явно спроектированной границы, не смены `127.0.0.1` на `0.0.0.0`.

Vite development config текущего среза проксирует `/api/v1`, `/api/ag-ui` и health, но описанный путь не добавляет Bearer так, как nginx. `VITE_VESTRACE_WORKSPACE_ID` и `VITE_VESTRACE_PRINCIPAL_ID` не заменяют authentication. Не добавляйте секретный `VITE_*` token в браузерный bundle как обход. F004 требует отдельного проверенного безопасного dev-proxy пути.

```bash
# Проверки фронтенда; не доказательство успешной авторизации/запуска backend.
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
```

Список scripts берётся из `apps/console/package.json`. В исходном срезе нет общего `npm test`; новые `test:memory`/e2e scripts относятся к будущему MW-03.

## Что не следует из наличия страницы

Маршруты Console уже существуют для Runs, Agents, Models, Connections и других областей. Это не означает, что все взаимодействия полноценны: 501, disabled actions и partial projections нужно показывать явно. `MemoryConsole.tsx` — существующий компонент представления, но полноценная страница библиотеки и редактор в main router пока отсутствуют.

## Первая цель изменения

Библиотека → карточка → история → исправление → перечитывание после reload. UI вызывает публичный API, не читает DB и не выбирает trusted actor. Текущий source text и editorial correction отображаются отдельно; sync conflict не замалчивается toast-сообщением об успехе.

Существующие styles/primitives переиспользуются. Не нужен отдельный frontend или визуальный graph editor для выполнения выбранной задачи. Полные proposed контракты находятся в [Console specification](../implementation/memory-workspace/04-console.md).

---
**Основание:** [R15: apps/console/vite.config.ts](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/vite.config.ts), [R16: apps/console/nginx.conf.template](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/nginx.conf.template), [S13: apps/console/src/memory/MemoryConsole.tsx](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/memory/MemoryConsole.tsx), [S14: apps/console/src/main.tsx](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/main.tsx), [S15: apps/console/src/sdk/client.ts](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk/client.ts), [S16: apps/console/package.json](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/package.json).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
