# 04. Console: библиотека, редактор и источники

## 4.1 Информационная архитектура

Добавить `/memory`, `/memory/:memoryId`, `/memory/:memoryId/history`, `/sources`, `/imports/:operationId`, `/conflicts/:conflictId`, `/exports/:operationId` в существующий BrowserRouter. Использовать нынешние AppLayout, ErrorBoundary, Surface/Button и design tokens. Новый визуальный бренд, второй SPA и графовый редактор не нужны.

`MemoryConsole.tsx` переиспользуется как представление detail и становится контейнером для содержимого/истории/редакторских действий. Предложенные новые route modules: MemoryPage, SourcesPage, ImportPage, SourceConflictPage. Export status можно отображать в существующем panel без отдельной тяжёлой subsystem.

## 4.2 Библиотека

Поиск и browse — разные режимы. Пустая строка в browse вызывает GET /v1/memories, не фиктивный semantic query. Фильтры: kind, status, collection, временной режим там, где он поддержан сервером. Порядок created_at/id, страницы до 50 по умолчанию. Никаких total counts, полученных из неавторизованного списка.

Состояния: loading, empty, ready, query-error, cursor-expired, forbidden, unavailable. Empty не используется вместо 503. При CursorExpired сохранять фильтры, объяснять обновление списка и начинать первую страницу. На смене workspace отменить pending fetch; поздний ответ прежнего workspace не попадает в новый UI.

## 4.3 Карточка и редактор

Detail показывает kind, status, содержимое, ordinal/state revision, заявленную classification, provenance_status, source links и reason последней правки. У percentage confidence подпись «заданная оценка», не «вероятность истинности».

Editor states: viewing → editing → submitting → applied; параллельные состояния conflict, failed, result-unknown. Текст draft не уничтожается при конфликте или сетевой ошибке. При 409 рядом показываются база, свежий current и draft. Для повторного намерения после разрешённого сравнения создаётся новый key. При result-unknown повторить прежний запрос тем же key; не обещать, что mutation не произошла.

Restore требует выбора разрешённой revision, preview текста и reason. Он создаёт новую revision. Classification поля readonly; попытка изменить их через devtools отклоняется сервером. Нельзя использовать restore для копирования denied истории в более доступную текущую запись.

Три origin обозначения: «из источника», «исправлено пользователем», «импортированная история». Legacy-unattributed provenance виден как ограничение, а не заполнен вымышленным actor/source.

## 4.4 Sources и импорт

В списке источников: имя/relative_path, latest source revision, effective memory revision, manual override, processing state. Абсолютный путь машины отправителя нигде не нужен.

Import UI шаги: выбрать файлы → явно задать collection/label/оценки → загрузить и получить durable preview → просмотреть new/update/unchanged/conflict/missing → выбрать items → подтвердить точный preview_revision → наблюдать progress.

До PreviewReady Apply выключен. Неизвестный исход загрузки не повод автоматически отправить другой batch. В progress показывать separately canonical applied, FTS readiness и embedding/retrieval readiness; «100% imported» не используется как «всё доступно в поиске».

Partial batch не скрывает item failures. Committed элементы отмечены неизменно; retry не должен создавать вторые копии. Отмена прекращает только непроведённые элементы. UI объясняет, что уже применённые изменения сохранятся.

## 4.5 Конфликт sync/edit

Показывать B — source, на основе которого редактировали; I — новый источник; M — current manual text. Доступны ровно `accept_source`, `keep_manual`, `merge` с явным текстом и reason. Нет фонового LLM merge.

При changed head или policy до подтверждения — 409/403, заново загрузить bases; не переиспользовать старую картинку diff как разрешение на новый content. Выбор keep_manual записывает управляемый override, а не изменяет I. При следующем изменении источника override снова требует проверки.

## 4.6 Безопасность rendering и доступность

Первый выпуск отображает содержимое как text/pre-wrap. Если позже добавляется Markdown renderer, raw HTML выключен, links только http/https с безопасным открытием; javascript/data/file links, remote images, scripts и embedded frames не исполняются. Никакого dangerouslySetInnerHTML с импортированным текстом.

Dialog имеет доступное название, focus trap и возврат фокуса; form error связан с полем; status не передаётся только цветом; progress объявляется ненавязчиво через aria-live. Во время submit доступен понятный статус, но не второй submit с новым ключом. Draft не сохраняется в localStorage; export скачивается только явным действием.

## 4.7 Проверка

Чистые reducer/view-model проверки выполняются Node test runner по аналогии с текущими mjs tests. Для реальных browser workflows MW-03 добавляет test-only Playwright с exact version в lockfile после согласования dependency diff; scripts `test:memory` и `test:e2e:memory` в baseline ещё нет. Typecheck/компонентный test не засчитываются как запуск browser+HTTP+PostgreSQL.

Обязательны keyboard-only edit/restore, конфликт двух браузерных сессий, workspace switch во время pending fetch, 503 вместо empty, deny исторического текста и импорт Markdown с HTML payload.
