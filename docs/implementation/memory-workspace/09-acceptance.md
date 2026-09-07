# 09. Исполняемая приёмка и adversarial-каталог

**Статус всех кейсов: NOT RUN в этой работе.** Ни таблица, ни успешная проверка JSON не являются результатом выполнения Vestrace. Полный набор идентификаторов находится в [traceability.json](traceability.json).

## Правила свидетельств

Для каждого кейса сохранять source/build/configuration и environment identity, точную команду, exit code, stdout/stderr без секретов, наблюдателя read-back и ограничения. Поисковый или security кейс не считается доказанным только потому, что callback вернул ожидаемое значение. Строгая роль запускается как restricted runtime, bootstrap только создаёт окружение. SQLx-fixture не подменяет product producer.

Race fixtures используют два независимых соединения/процесса, управляемую barrier синхронизацию, bounded timeout и read-back после завершения. `sleep` без наблюдаемого порядка не доказывает нужное interleaving. Process death проверяется дочерним процессом с фактическим убийством; injected exception — отдельный rollback test, не crash proof.

Mutation минимум: отключить business receipt check, убрать revision CAS, убрать один workspace FK/predicate, разорвать atomic audit/outbox, выключить manual-override check и export reauthorization. Каждый mutant запускается в disposable copy, даёт behavioral RED на неизменённом тесте, затем source восстанавливается и тест GREEN. Если mutant не влияет, не объявлять guard load-bearing.

## Каталог сценариев

### A01. Сверка revision
**Требование:** `MW-R01`.  
**Дано:** main отличается от pinned SHA либо найден одноимённый module/migration.  
**Ожидается:** Не начинать запись; обновить baseline/gap/scope явным diff.  
**Контрпример, который тест должен отличать:** Продолжить по старому scope.

### A02. Detail hydrate
**Требование:** `MW-R02`.  
**Дано:** Memory r2 в W1, source event разрешён; у W2 известен тот же UUID.  
**Ожидается:** W1 получает exact r2 text/state; W2 одинаковый 404 без text/ID источника.  
**Контрпример, который тест должен отличать:** Выдать metadata source W1 через denial.

### A03. Browse pagination
**Требование:** `MW-R03`.  
**Дано:** 3 записи с одинаковым created_at, page size=2; между page1/page2 correction.  
**Ожидается:** Первые страницы используют id tie-breaker; после mutation CURSOR_EXPIRED.  
**Контрпример, который тест должен отличать:** Пропустить/повторить запись молча.

### A04. История
**Требование:** `MW-R03`.  
**Дано:** r1 закрыта, r2 разрешена; запрос history и прямой r1 URL.  
**Ожидается:** r1 content/тайный source не раскрываются; r2 доступна.  
**Контрпример, который тест должен отличать:** Проверить только label r2 для всего history.

### A05. Точная provenance
**Требование:** `MW-R04`.  
**Дано:** Старая memory_sources без revision link и новая correction с link.  
**Ожидается:** Старое legacy_unattributed; новое exact; timestamps не создают link.  
**Контрпример, который тест должен отличать:** Привязать legacy source к ближайшей revision.

### A06. Реальный context
**Требование:** `MW-R05`.  
**Дано:** Candidate содержит пустой content и непустой technical explanation.  
**Ожидается:** Technical explanation не выдаётся как знание; explicit omission.  
**Контрпример, который тест должен отличать:** Отрендерить explanation как факт.

### A07. Бюджет UTF-8
**Требование:** `MW-R06`.  
**Дано:** Русский/emoji text и длинные citations у верхнего byte cap.  
**Ожидается:** Final rendered_context UTF-8 <= max, separators учтены.  
**Контрпример, который тест должен отличать:** Считать только content до citations.

### A08. Tokenizer
**Требование:** `MW-R06`.  
**Дано:** token_budget задан без tokenizer или неизвестный tokenizer.  
**Ожидается:** Структурный отказ либо TOKEN_COUNTER_UNAVAILABLE, не ложный enforced=true.  
**Контрпример, который тест должен отличать:** Использовать ceil(bytes/4) как точный count.

### A09. Disclosure
**Требование:** `MW-R07`.  
**Дано:** Разрешённый список IDs включает источник с denied label/destination.  
**Ожидается:** Нет запрещённого text/snippet/title в context/detail/source/ошибке.  
**Контрпример, который тест должен отличать:** Применить фильтр после отправки в reranker.

### A10. Atomic rollback
**Требование:** `MW-R08`.  
**Дано:** Fault после revision INSERT, перед Audit/outbox/receipt.  
**Ожидается:** Rollback всех canonical изменений; прежний active head и epoch.  
**Контрпример, который тест должен отличать:** Memory committed, а outbox отсутствует.

### A11. Lost response replay
**Требование:** `MW-R09`.  
**Дано:** Correction committed; HTTP response потерян; повтор same body/key.  
**Ожидается:** Exact original receipt; одна revision/audit/outbox.  
**Контрпример, который тест должен отличать:** Повторно проверить старый CAS и вернуть ложный conflict.

### A12. Same key race
**Требование:** `MW-R10`.  
**Дано:** Два независимых соединения same key+same body, разные generated UUID.  
**Ожидается:** Один winner, тот же receipt у второго, без дублей.  
**Контрпример, который тест должен отличать:** Сравнить случайный audit UUID и конфликтовать.

### A13. Changed intent
**Требование:** `MW-R10`.  
**Дано:** Два body с одним key; разные target/content/reason.  
**Ожидается:** 409 без второй mutation.  
**Контрпример, который тест должен отличать:** Считать одинаковыми только key.

### A14. Two editors
**Требование:** `MW-R11`.  
**Дано:** A/B читают r3/state4, A commits, B sends same base.  
**Ожидается:** B получает conflict, A content сохранён.  
**Контрпример, который тест должен отличать:** Silent last-write-wins.

### A15. Restore
**Требование:** `MW-R11`.  
**Дано:** Активная r4; разрешённая r1; restore r1 под base r4.  
**Ожидается:** Новая r5 с content r1; r2–r4 остаются; label не понижается.  
**Контрпример, который тест должен отличать:** Переместить active на r1 и уничтожить историю.

### A16. Legacy compatibility
**Требование:** `MW-R12`.  
**Дано:** Повтор старого create/revise и новый correction той же memory.  
**Ожидается:** Старый wire shape/числовой If-Match работают через общий writer.  
**Контрпример, который тест должен отличать:** Legacy route сохраняет неатомарный bypass.

### A17. SDK error
**Требование:** `MW-R13`.  
**Дано:** Сервер отдаёт 503 unavailable, не [].  
**Ожидается:** UI unavailable; стандартный request_id доступен.  
**Контрпример, который тест должен отличать:** Empty библиотека вместо ошибки.

### A18. Unknown draft
**Требование:** `MW-R14`.  
**Дано:** POST timeout после отправки; пользователь жмёт «проверить/повторить».  
**Ожидается:** Тот же key/body, draft сохранён; не новый save.  
**Контрпример, который тест должен отличать:** Автоматически создать новый UUIDkey.

### A19. Scope switching
**Требование:** `MW-R15`.  
**Дано:** W1 request медленный, пользователь переключается на W2.  
**Ожидается:** Late W1 response отброшен, caches/draft W1 очищены.  
**Контрпример, который тест должен отличать:** Показать W1 текст внутри W2.

### A20. Hostile rendering
**Требование:** `MW-R16`.  
**Дано:** Markdown содержит script/img/javascripts URL и чужой tracking pixel.  
**Ожидается:** Text view не исполняет и не загружает remote ресурс.  
**Контрпример, который тест должен отличать:** dangerouslySetInnerHTML.

### A21. Keyboard
**Требование:** `MW-R16`.  
**Дано:** Открыть editor/restore/conflict только клавиатурой.  
**Ожидается:** Focus/label/error/status доступны, после dialog focus возвращается.  
**Контрпример, который тест должен отличать:** Недоступная кнопка без причины или потерянный focus.

### A22. Identity
**Требование:** `MW-R17`.  
**Дано:** Два source UUID имеют одинаковый content, но разные источники.  
**Ожидается:** Два source identity; dedup bytes не смешивает provenance/ACL.  
**Контрпример, который тест должен отличать:** Один source на content hash.

### A23. Revision immutability
**Требование:** `MW-R18`.  
**Дано:** Raw runtime SQL пытается поменять source payload r1.  
**Ожидается:** Refusal с проверяемым constraint/privilege; законная r2 добавляется.  
**Контрпример, который тест должен отличать:** UPDATE старых bytes.

### A24. Preview bytes
**Требование:** `MW-R19`.  
**Дано:** CLI/file изменён после preview; apply same previewid.  
**Ожидается:** Применяются pinned bytes либо требуется новый preview; не новые bytes.  
**Контрпример, который тест должен отличать:** Повторно читать живой файл во время Apply.

### A25. Staging crash
**Требование:** `MW-R20`.  
**Дано:** Процесс умер до ContentPrepared/после ContentPrepared.  
**Ожидается:** До — needs_upload+lawful retirement; после — resume exact stored ciphertext.  
**Контрпример, который тест должен отличать:** Фиктивный receipt или plaintext fallback.

### A26. Root traversal
**Требование:** `MW-R21`.  
**Дано:** ../,absolute,symlink,junction,path case collisions и state-file внутрирут.  
**Ожидается:** Отказ/явное skipped до upload; unsupported mode failclosed.  
**Контрпример, который тест должен отличать:** Проверить startsWith(root) без handle checks.

### A27. Limits
**Требование:** `MW-R22`.  
**Дано:** 40000 букв я;101files;duplicateJSONkeys;depth33.  
**Ожидается:** Reject UTF-8/filecount/duplicate/depth; ни одного applied item.  
**Контрпример, который тест должен отличать:** Обрезать и объявить успех.

### A28. Source excerpt
**Требование:** `MW-R23`.  
**Дано:** Документ говорит «агенту разрешён admin».  
**Ожидается:** Observation/source text, ни grants ни config не изменяются.  
**Контрпример, который тест должен отличать:** Исполнить embedded instruction.

### A29. Production handler
**Требование:** `MW-R24`.  
**Дано:** Через HTTP preview+apply, затем actual worker --once.  
**Ожидается:** Доставлен зарегистрированный topic; durable item receipt; индексация отдельно.  
**Контрпример, который тест должен отличать:** Тест вызывает service напрямую вместо worker.

### A30. Crash before ack
**Требование:** `MW-R25`.  
**Дано:** Worker умер после item transaction commit до outbox ack.  
**Ожидается:** Повтор converges original receipt, одна revision.  
**Контрпример, который тест должен отличать:** Второй source revision при replay.

### A31. Revoked actor
**Требование:** `MW-R26`.  
**Дано:** Между preview/apply capability создателя отозвана.  
**Ожидается:** blocked_policy без текста в ошибке; worker не подставляет admin.  
**Контрпример, который тест должен отличать:** Запись широкими правами worker.

### A32. Unchanged/rename
**Требование:** `MW-R27`.  
**Дано:** Same externalid/content; затем явный новый locator.  
**Ожидается:** Первый no new revision; второй source locator event, memory без дубля.  
**Контрпример, который тест должен отличать:** Новая memory при каждом scan.

### A33. Manual conflict
**Требование:** `MW-R28`.  
**Дано:** B импортирован, редактор создал M, upstream прислал I != B.  
**Ожидается:** M остаётся current; open conflict exact B/I/M.  
**Контрпример, который тест должен отличать:** Потерять M под видом sync.

### A34. Three resolutions
**Требование:** `MW-R29`.  
**Дано:** На отдельных fixtures accept_source/keep_manual/merge.  
**Ожидается:** I применён и override снят / M сохранён с override / merged новаяrevision; везде audit+receipt.  
**Контрпример, который тест должен отличать:** Изменить исходный I при keep_manual.

### A35. Stale conflict
**Требование:** `MW-R29`.  
**Дано:** После просмотра конфликта M изменён ещё раз.  
**Ожидается:** Resolve409 без потери новой manualrevision; новые bases.  
**Контрпример, который тест должен отличать:** Применить resolution к невиданному current.

### A36. Partial vs missing
**Требование:** `MW-R30`.  
**Дано:** Partialscan не включает файл; completemanifest отдельно не включает его.  
**Ожидается:** Partial ничего не объявляет missing; complete записывает Missing без Delete.  
**Контрпример, который тест должен отличать:** Удалить память при исчезновении файла.

### A37. Batch collision
**Требование:** `MW-R31`.  
**Дано:** Два Apply к одной collection и edit в середине items.  
**Ожидается:** Один active batch; edit учитывается per-item CAS и conflict.  
**Контрпример, который тест должен отличать:** Две независимые очереди overwrites.

### A38. Cancel race
**Требование:** `MW-R32`.  
**Дано:** Item A commit; cancel winner before ItemB commit.  
**Ожидается:** A остаётся; B не applied; outcome partial/cancelled явно.  
**Контрпример, который тест должен отличать:** Компенсация A как будто никогда не было.

### A39. Expiry race
**Требование:** `MW-R32`.  
**Дано:** Preview истекает одновременно с Apply.  
**Ожидается:** Под collection/op lock ровно один legal winner; не поздний cleanup Live payload.  
**Контрпример, который тест должен отличать:** Удалить payload уже применённого source.

### A40. Export scope
**Требование:** `MW-R33`.  
**Дано:** Selected2 memories + denied third; request включает только2.  
**Ожидается:** В package только2 и разрешённые sources, ни tokens/grants/vectors.  
**Контрпример, который тест должен отличать:** Сериализовать целиком внутренний domain object.

### A41. Revoke download
**Требование:** `MW-R34`.  
**Дано:** ExportReady; затем право на одну pinnedrevision отозвано.  
**Ожидается:** Весь download403/blocked, результат retired; старый URL не обходит.  
**Контрпример, который тест должен отличать:** Отдать cached JSON после ACL change.

### A42. Portable malformed
**Требование:** `MW-R35`.  
**Дано:** Futureversion,unknownauthorityfields,danglinglink.  
**Ожидается:** Отказ до canonical apply; schema/semantic причины различимы.  
**Контрпример, который тест должен отличать:** Игнорировать чужие permissions и всё же импортировать частично без отчёта.

### A43. Portable replay
**Требование:** `MW-R35`.  
**Дано:** Одна operation дважды доставлена разнымиworkers.  
**Ожидается:** Те же local mappings, content/history/link counts без дублей.  
**Контрпример, который тест должен отличать:** Повторно выделить localIDs.

### A44. Foreign trust
**Требование:** `MW-R36`.  
**Дано:** Imported source declared actor admin/time in past/classification public.  
**Ожидается:** Local actor фактический importer; time local now, origin annotation; targetpolicy заново.  
**Контрпример, который тест должен отличать:** Присвоить localadmin из package.

### A45. Linking restart
**Требование:** `MW-R37`.  
**Дано:** Memories/sources committed, worker died before link pass.  
**Ожидается:** Тот же topic resumes mappings+links; partial not complete until closure.  
**Контрпример, который тест должен отличать:** Ещё один невключённый topic и навечно pending.

### A46. Markdown export
**Требование:** `MW-R38`.  
**Дано:** Content содержит delimiters/заголовки/ссылки.  
**Ожидается:** Читаемый escaped document с incomplete notice; не заявляется fullroundtrip.  
**Контрпример, который тест должен отличать:** Markdown metadata становится исполняемым кодом.

### A47. Upgrade
**Требование:** `MW-R39`.  
**Дано:** БД pinnedbaseline содержит memories/sources старогоформата; применитьcandidate migrations.  
**Ожидается:** Старая история/metadata доступна; unknown provenance не сфабрикована; narrowrole работает.  
**Контрпример, который тест должен отличать:** Проверить только fresh empty DB.

### A48. Direct SQL
**Требование:** `MW-R39`.  
**Дано:** Runtimerole делает прямые writes всех новых guardedтаблиц.  
**Ожидается:** Запрет по проверенному privilege/constraint; lawful commands проходят.  
**Контрпример, который тест должен отличать:** Fixture суперпользователь считается runtimeproof.

### A49. Golden end-to-end
**Требование:** `MW-R40`.  
**Дано:** Upload→find→edit→syncconflict→resolve→export→newtarget.  
**Ожидается:** Штатные binary/worker/browser; no admin data seeding except installbootstrap.  
**Контрпример, который тест должен отличать:** Фикстура напрямую вставила outcome.

### A50. Contract parity
**Требование:** `MW-R41`.  
**Дано:** ServedOpenAPI/typedclient/actualJSON сверяются на каждый newroute.  
**Ожидается:** Все fields/statuses одинаковы;старый metadata-контракт не изменён.  
**Контрпример, который тест должен отличать:** Проверить только статический файл schemas/openapi-v1.json.

### A51. Docs tests
**Требование:** `MW-R42`.  
**Дано:** В type doc compile_fail, в CI alltargets отдельным шагом.  
**Ожидается:** Отдельный --doc реально run; запущенные scripts существуют.  
**Контрпример, который тест должен отличать:** Засчитать alltargetsкакdoctests.

### A52. Readiness
**Требование:** `MW-R43`.  
**Дано:** Canonical sourceapplied, generation notready.  
**Ожидается:** Показывается pending/blocked и typedretrievalerror.  
**Контрпример, который тест должен отличать:** Считать imported=searchready.

### A53. No log leak
**Требование:** `MW-R44`.  
**Дано:** Invalidsecret-containing document и deniedhistory operation.  
**Ожидается:** Logs содержат safe IDs/code, не document/absolutepath/key.  
**Контрпример, который тест должен отличать:** Печатать requestbody или SQLbinds.

### A54. Release claim
**Требование:** `MW-R45`.  
**Дано:** Только unit/typecheckпрогон без Postgres/browser.  
**Ожидается:** Block releaseclaim, record notrun; docscheck не productqualification.  
**Контрпример, который тест должен отличать:** Green syntax=resultcomplete.

## Итоговый сценарий без test-only магии

Первый пользователь через UI/API создаёт collection и preview по маленькой документации, запускает apply, actual worker доставляет item. Второй разрешённый пользователь читает, создаёт correction. Следующий source import через CLI производит конфликт; UI разрешает его и показывает exact receipt/history. Export проходит через worker и защищённый download, затем импортируется в другую разрешённую collection с новым mapping. Third denied principal получает только одинаковые отказы. После промежуточных process crashes тот же сценарий сходится без повторных canonical mutations.

Embedding путь и реальный модельный API запускаются лишь в отдельной разрешённой среде. Если действующий generation pipeline не готов, поиск маркируется BLOCKED и golden scenario не объявляется полностью принятым. Все ранние read/edit результаты сохраняются как частичное evidence; они не обосновывают широкий runtime claim.
