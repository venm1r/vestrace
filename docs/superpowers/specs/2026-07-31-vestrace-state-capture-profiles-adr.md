# ADR: Vestrace State capture profiles

**Статус:** предложено для письменного утверждения  
**Дата:** 2026-07-31  
**Репозиторий:** `venm1r/vestrace`  
**Связанный boundary:** `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`  
**Базовый контракт:** `docs/superpowers/plans/2026-07-31-vestrace-h10-observability-evaluation.md`  
**Связанный lifecycle данных:** `docs/superpowers/plans/2026-07-31-vestrace-h6-context-artifact-runtime.md`

> Этот ADR является documentation-only решением. Он не разрешает менять Rust-код, создавать миграции, включать capture в production, запускать exporters или сохранять дополнительные payload.

## 1. Контекст

В ходе State Engine reconciliation были утверждены четыре понятных оператору уровня журналирования:

```text
minimal
operational
reproducible
forensic
```

H10 уже определяет более точный внутренний контракт capture modes:

```text
Disabled < MetadataOnly < StructuredOnly < Redacted < Full
```

Эти две модели решают разные задачи.

- Capture mode является точным техническим решением о допустимой форме конкретного захвата.
- Capture profile является удобным операторским preset, который задаёт базовые предпочтения для workspace, deployment, Run или diagnostic session.

Попытка заменить H10 modes четырьмя профилями привела бы к потере важных состояний, особенно `Disabled`, и смешала бы конфигурационное намерение с итоговым policy decision.

## 2. Решение

Vestrace сохраняет H10 capture modes без изменения:

```text
Disabled
MetadataOnly
StructuredOnly
Redacted
Full
```

Дополнительно вводятся четыре операторских State capture profiles:

```text
Minimal
Operational
Reproducible
Forensic
```

Профиль является версионируемым набором базовых предпочтений. Он не является новым event taxonomy, уровнем секретности, retention class или разрешением на чтение данных.

Итоговый capture mode для каждого capture point вычисляется текущей policy и может быть только таким же или менее раскрывающим, чем baseline профиля.

## 3. Нормативное отображение

Базовое отображение:

| State capture profile | H10 baseline mode | Основное назначение |
|---|---|---|
| `Minimal` | `MetadataOnly` | эксплуатационный минимум, hashes, references, revisions, outcomes и обязательный audit |
| `Operational` | `StructuredOnly` | диагностика состояния и производительности без произвольного пользовательского содержимого |
| `Reproducible` | `Redacted` | воспроизводимость с очищенным содержимым, точными revision/reference manifests и omissions |
| `Forensic` | `Full` | расследование инцидента при явном разрешении policy и строгом lifecycle содержимого |

`Disabled` не имеет отдельного profile. Это итоговый H10 mode для конкретного optional capture point, когда захват запрещён или не нужен.

Профиль не гарантирует соответствующий mode. Например:

```text
Forensic profile
∩ Restricted data policy
∩ exporter allows MetadataOnly
∩ secret-scan result
= MetadataOnly or Disabled for that capture point
```

## 4. Порядок раскрытия

H10 ordering остаётся нормативным:

```text
Disabled < MetadataOnly < StructuredOnly < Redacted < Full
```

Profile ordering используется только как операторская шкала:

```text
Minimal < Operational < Reproducible < Forensic
```

Нельзя сравнивать profile и mode как одно и то же enum-значение. Resolution обязан сначала развернуть profile в baseline policy input, а затем получить итоговый H10 mode.

## 5. Обязательные инварианты

### 5.1 Канонические события не зависят от профиля

State capture profile не может:

- удалить обязательный H1 `RunEvent`;
- изменить event kind или payload schema канонического события;
- отключить обязательный H2 authorization/accounting fact;
- отключить обязательный H6 lifecycle/provenance fact;
- отключить обязательный H8 credential-use audit;
- превратить telemetry в источник истины;
- изменить Run, approval, budget или Artifact state.

### 5.2 Security audit сохраняется

Даже `Minimal` и итоговый `Disabled` для optional content capture не отключают:

- content-free security audit intents и records;
- policy decision references;
- approval bindings;
- accounting facts;
- hashes и revision references, когда они обязательны;
- validation outcomes;
- operational health counters с bounded labels.

### 5.3 Policy может только уменьшить раскрытие

Текущая policy может понизить baseline:

```text
Full → Redacted → StructuredOnly → MetadataOnly → Disabled
```

Policy не может повысить capture выше baseline профиля без отдельного явного override, который сам проходит H2 authorization, versioning, expiry и audit.

Classification, workspace policy, principal policy, exporter binding, retention, legal hold, secret detection, quarantine и purge имеют приоритет над profile preference.

### 5.4 Hidden reasoning не сохраняется

Ни один profile и ни один H10 mode не разрешает сохранять:

- скрытый chain-of-thought;
- provider reasoning-token content;
- private scratchpad модели;
- внутренние safety deliberations;
- секреты или credential bytes;
- unrestricted external content без classification и bounds.

Для воспроизводимости сохраняются структурированные решения, inputs/outputs, exact references, policy facts, hashes, omissions и validation records.

### 5.5 Capture bytes принадлежат H6

Любые optional bytes, появившиеся из `Redacted` или `Full`, становятся governed H6 Artifact revision либо representation.

Они обязаны иметь:

- workspace ownership;
- source capture record;
- content hash;
- classification;
- exact producer/generator revision;
- quarantine и inspection state;
- retention policy;
- purge propagation;
- export authorization.

H10 не создаёт отдельный blob store.

## 6. Семантика профилей

### 6.1 Minimal

Цель — сохранить минимально достаточную эксплуатационную и аудиторскую доказательность.

Baseline:

```text
MetadataOnly
```

Обычно сохраняются:

- IDs и bounded correlation references;
- timestamps;
- stable kinds/statuses;
- exact component revisions;
- request/result hashes;
- usage и cost accounting;
- policy/approval references;
- validation и verification outcomes;
- error categories без произвольных external messages;
- explicit omission reasons.

Не сохраняются optional prompt, response, tool payload или document excerpts.

### 6.2 Operational

Цель — поддержать штатную эксплуатацию, диагностику latency, retries, queueing и failure categories.

Baseline:

```text
StructuredOnly
```

Дополнительно могут сохраняться bounded structured fields:

- normalized operation categories;
- selected routing reasons из закрытого enum;
- retry/reconciliation state;
- budget counters;
- schema validation summaries;
- redaction counters;
- bounded tool/model outcome metadata.

Arbitrary user content, raw external bodies и unrestricted errors не включаются.

### 6.3 Reproducible

Цель — дать достаточно данных для контролируемого replay, regression analysis и воспроизведения решения без секретов и скрытых рассуждений.

Baseline:

```text
Redacted
```

Дополнительно требуются, когда policy разрешает:

- exact source revision manifests;
- ContextSnapshot references и rendered hash;
- selected memory/evidence/artifact revision references;
- normalized request and output-contract references;
- deterministic tool fixture or recorded-observation references;
- redacted prompt/output capture Artifacts;
- omissions и причины redaction;
- provider/tool/model version facts;
- replayability assessment.

Недостаточный capture приводит к `NotReplayable` или `Inconclusive`, а не к восстановлению отсутствующего содержимого догадкой.

### 6.4 Forensic

Цель — ограниченное расследование инцидентов, integrity failures, policy violations или data-leak hypotheses.

Baseline:

```text
Full
```

`Full` разрешён только при одновременном выполнении условий:

- profile explicitly activated;
- H2 authorization разрешает exact scope;
- workspace/deployment policy допускает этот тип содержимого;
- H6/H8 classification и secret policy допускают capture;
- capture имеет bounded duration и scope;
- retention и purge определены до начала;
- exporter binding не шире разрешённого mode;
- activation и deactivation записаны в audit.

Forensic profile не является режимом «записывать всё». Секреты, hidden reasoning и запрещённое policy содержимое остаются запрещёнными.

## 7. Scope и precedence

Profile может быть задан на уровнях:

```text
Deployment default
→ Workspace profile revision
→ Agent/Workflow constraint
→ Run profile selection
→ bounded diagnostic override
→ exact capture-point policy decision
→ exporter binding cap
```

Итог выбирается по более ограничительному результату.

Нижний уровень не расширяет верхний ceiling, кроме отдельного diagnostic override, который обязан:

- иметь exact authorized principal;
- быть связанным с workspace и optional Run;
- иметь цель;
- иметь `valid_from`/`valid_until`;
- иметь maximum mode;
- иметь exporter scope;
- иметь retention class;
- быть revocable;
- создавать security audit fact.

## 8. Activation и versioning

Profile definition и workspace selection используют immutable revisions.

Изменение mapping, exporter caps, retention defaults или allowed diagnostic override создаёт новую revision.

Активный Run фиксирует:

- selected profile revision;
- policy bundle revisions;
- capture policy revision;
- exporter binding revisions;
- per-capture resolved mode;
- omission/redaction reasons.

Изменение default profile не переписывает историю и не повышает capture уже выполняющегося Run автоматически.

Current policy применяется к новым capture operations внутри старого Run и может понизить допустимый mode.

## 9. Export boundary

Exporter получает не profile name, а уже разрешённый bounded capture record и resolved mode.

Exporter binding задаёт maximum mode:

```text
exported_mode = min(resolved_capture_mode, exporter_maximum_mode)
```

Экспорт обязан повторно проверить:

- current authorization;
- classification;
- destination policy;
- retention;
- redaction state;
- secret-scan state;
- Artifact lifecycle;
- exact exporter revision.

Profile не является разрешением на экспорт.

## 10. Retention и purge

Profile может предлагать default retention class, но H6/H10 policy определяет фактический срок.

Нормативные ожидания:

| Profile | Типичный default |
|---|---|
| `Minimal` | длительное хранение content-free facts согласно audit policy |
| `Operational` | ограниченное хранение structured diagnostics |
| `Reproducible` | retention, достаточный для regression/replay window |
| `Forensic` | короткий явный срок либо legal-hold policy |

Hard purge optional capture content распространяется на:

- Artifact bytes;
- derived representations;
- chunks и embeddings;
- cached excerpts;
- temporary export copies;
- replay fixtures, содержащие эти bytes.

Сохраняется только минимальный content-free факт purge и невозможности воспроизведения.

## 11. Failure semantics

### 11.1 Capture policy unavailable

Если итоговую policy нельзя получить:

- mandatory content-free audit сохраняется допустимым безопасным способом;
- optional capture получает `Disabled`;
- Run не повышает раскрытие на основании fallback;
- создаётся bounded operational error fact.

### 11.2 Capture storage unavailable

Сбой optional H6 capture storage:

- не должен автоматически проваливать production Run;
- фиксируется как `CaptureUnavailable` или equivalent bounded outcome;
- снижает replayability;
- не приводит к сохранению bytes в ordinary logs или временный небезопасный backend.

Если exact capture является обязательным verification/compliance criterion, owning operation следует явной policy: pause, fail closed или request intervention.

### 11.3 Redaction failed

Если Redacted capture нельзя безопасно сформировать:

- запрещено сохранять исходный Full payload как fallback;
- итоговый mode понижается до `StructuredOnly`, `MetadataOnly` или `Disabled`;
- сохраняется redaction failure category без чувствительного содержимого.

### 11.4 Forensic expiry

После истечения diagnostic override:

- новые captures немедленно возвращаются к обычному profile ceiling;
- ранее созданные Artifacts следуют заранее зафиксированным retention/purge rules;
- продление требует нового authorization decision.

## 12. Public и UI semantics

Public DTO может показывать:

- selected profile;
- profile revision;
- resolved mode для конкретного capture record;
- причины понижения;
- replayability status;
- retention/purge state.

Public surface не показывает:

- raw policy rules, если они чувствительны;
- secret detection details, позволяющие восстановить секрет;
- backend storage references;
- hidden reasoning;
- payload, который не разрешён текущему principal.

UI обязана ясно различать:

```text
requested profile
resolved capture mode
content availability
export authorization
```

## 13. Rejected alternatives

### 13.1 Заменить H10 modes четырьмя профилями

Отклонено: теряется `Disabled`, затрудняется exporter cap и смешивается intent с enforcement result.

### 13.2 Считать profile гарантией capture

Отклонено: это позволило бы profile обойти classification, secrets, retention и current policy.

### 13.3 Forensic означает unrestricted logging

Отклонено: нарушает data minimization, secret boundary и запрет hidden reasoning.

### 13.4 Хранить Full payload прямо в PostgreSQL

Отклонено: optional capture bytes принадлежат H6 Artifact lifecycle и должны быть purgeable отдельно от content-free records.

### 13.5 Отключать audit в Minimal

Отклонено: audit является обязательным security fact и не относится к optional diagnostic capture.

## 14. Consequences

Положительные последствия:

- сохраняется точный H10 enforcement contract;
- операторы получают понятные presets;
- канонические события и audit не зависят от уровня диагностики;
- forensic capture остаётся policy-governed;
- replayability становится объяснимой;
- profile может эволюционировать без изменения event semantics.

Стоимость решения:

- UI/API должны показывать profile и resolved mode отдельно;
- policy evaluation выполняется для каждого capture point;
- тесты должны покрывать profile-to-mode downgrade;
- exporter и retention contracts должны учитывать обе сущности.

## 15. Acceptance scenarios

ADR считается достаточно определённым, если последующий implementation plan содержит tests для сценариев:

1. `Minimal` сохраняет mandatory audit при optional `Disabled`.
2. `Operational` не сохраняет arbitrary prompt/tool payload.
3. `Reproducible` создаёт redacted H6 capture Artifact с exact references.
4. `Forensic` без authorization понижается, а не повышается до `Full`.
5. Restricted classification понижает `Forensic` до допустимого mode.
6. Exporter с cap `MetadataOnly` не получает Redacted/Full content.
7. Redaction failure не приводит к Full fallback.
8. Profile change не переписывает старые capture records.
9. Expired override прекращает новые Full captures.
10. Hard purge удаляет capture bytes и оставляет content-free tombstone.
11. Missing capture приводит к `NotReplayable`, а не fabricated replay input.
12. Ни один profile не сохраняет hidden reasoning или secret material.

## 16. Documentation integration

После утверждения этого ADR:

- H10 implementation plan должен ссылаться на него как на normative profile mapping;
- H6 plan остаётся владельцем capture Artifact lifecycle;
- H11 public surfaces должны различать profile и resolved mode;
- cross-plan normalization audit обязан проверить отсутствие утверждений, что profile меняет канонический journal или mandatory audit.

## 17. Итоговое решение

> **State capture profiles являются удобными версионируемыми presets над H10 capture policy. Они задают baseline предпочтение, но не изменяют канонические события, не отключают обязательный audit, не повышают раскрытие выше policy и не разрешают сохранять hidden reasoning или secret material.**
