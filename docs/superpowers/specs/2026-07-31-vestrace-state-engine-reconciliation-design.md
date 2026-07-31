# Vestrace State Engine — reconciliation design

**Статус:** предложено для письменного просмотра  
**Дата:** 2026-07-31  
**Репозиторий:** `venm1r/vestrace`  
**Ветка документа:** `agent/state-engine-reconciliation`  
**Базовые документы:**

- `docs/superpowers/specs/2026-07-31-vestrace-v0.1-design.md`;
- `docs/superpowers/specs/2026-07-31-vestrace-harness-v0.2-design.md`;
- планы H1–H11B в `docs/superpowers/plans/`.

> Этот документ является planning artifact. Он не разрешает создавать миграции, Rust-код, новый runtime-процесс, отдельную базу данных или implementation branch.

## 1. Назначение

В ходе обсуждения были приняты 56 решений о специализированном State Engine для Vestrace. Большая часть этих решений уже покрыта архитектурой Vestrace v0.1 и Harness v0.2, однако первоначальная формулировка предполагала отдельную параллельную подсистему с собственной моделью `Run → Task → Step → Action → Attempt` и собственным API.

Такой параллельный стек противоречил бы уже утверждённым контрактам Vestrace:

- Vestrace v0.1 является самостоятельным memory-first Rust-сервисом;
- Harness v0.2 расширяет его durable execution runtime;
- H1 владеет `AgentRun`, `RunStep`, `RunEvent`, checkpoint, replay, lease и work item;
- H2 владеет capabilities, policy, approvals и budgets;
- H3 и H4 владеют model/tool execution;
- H5 владеет planning, delegation, SubRun и remote-agent execution;
- H6 владеет context snapshots, artifacts, evidence, Artifact mounts, retention и memory consolidation;
- H7 владеет conversations, human continuations, public events и triggers;
- H8 владеет connections, secrets и credential leases;
- H10 владеет telemetry, security audit, evaluation, verification, safe replay и explanations;
- H11/H11A/H11B владеют публичными product/API/AG-UI/web boundaries.

Цель документа — сохранить полезные решения из обсуждения, исключить дублирование и определить точное нормативное место каждого решения.

## 2. Итоговое архитектурное решение

### 2.1. State Engine не является новым продуктом или сервисом

`Vestrace State Engine` допускается использовать как внутренний архитектурный термин для совокупности durable-state контрактов Harness, но не как:

- отдельный продукт;
- отдельную базу данных;
- отдельный сетевой сервис;
- второй event store;
- второй orchestration runtime;
- параллельный набор таблиц `runs/tasks/steps/actions`;
- новый публичный API.

Нормативная формулировка:

> **Vestrace State Engine — внутреннее название durable execution-state boundary, реализуемой существующими H1–H10 application ports, PostgreSQL aggregates, journals, checkpoints, policies, artifacts, memory и projections.**

### 2.2. Существующие агрегаты остаются авторитетными

Исполнительная модель остаётся:

```text
AgentRun
├── active ExecutionPlanRevision
├── RunStep[]
├── RunEvent[]
├── RunCheckpoint[]
└── typed execution references
    ├── ModelExecution
    ├── ToolInvocation
    ├── SandboxSession
    ├── AgentRun as SubRun
    ├── RemoteAgentInvocation
    ├── HumanRequest
    └── VerificationAttempt
```

Не вводятся новые универсальные агрегаты `Task`, `Action` и `Attempt`, если их семантика уже принадлежит конкретному Horizon. Попытки выполнения хранятся владельцем операции: model runtime, tool runtime, remote invocation, verification или `RunStep`.

### 2.3. PostgreSQL — источник истины, application ports — граница доступа

PostgreSQL остаётся единственным authoritative persistence backend, но SQL не становится доменным или agent-facing API.

- Domain/Application не зависят от SQLx.
- Запись выполняется только через Vestrace-owned application ports.
- Агенты не выполняют произвольный write SQL.
- Ограниченные read-only operational views могут использоваться административными и аналитическими адаптерами под RLS и policy.
- HTTP, MCP, SDK, AG-UI и CLI остаются адаптерами над теми же application services.

## 3. Классификация

| Код | Значение |
|---|---|
| **A** | Полностью совпадает с существующим нормативным контрактом. |
| **N** | Намерение сохраняется, но нормализуется к существующей модели или терминологии. |
| **E** | Полезное дополнение, для которого нужен отдельный amendment/ADR после утверждения. |
| **C** | Конфликтует с утверждённой архитектурой; действующий контракт имеет приоритет. |

## 4. Reconciliation matrix

| № | Принятое решение | Класс | Нормативное разрешение | Владелец |
|---:|---|:---:|---|---|
| 1 | State Engine — внутренний компонент Vestrace | N | Термин относится только к durable execution-state boundary Harness v0.2. Vestrace v0.1 остаётся самостоятельным продуктом. | v0.1, Harness v0.2, этот документ |
| 2 | MVP: один пользователь, один локальный компьютер | N | Personal/local deployment является первым operational slice, но schema, RLS и contracts не сужаются до single-principal, поскольку Harness также поддерживает Team и Embedded deployment. | Foundation, Harness v0.2 |
| 3 | Workspace изолирован; общая память подключается явно | E | Изоляция уже обеспечена forced RLS и scoped transactions. Но v0.1 прямо запрещает межпространственную память: `global` означает только текущий workspace. Явный cross-workspace memory sharing требует отдельного grant/mount design. | v0.1 memory; amendment F |
| 4 | Append-only события плюс materialized current state | A | H1 RunEvent является каноническим журналом logical Run mutations; H10 operational views являются производными и могут отставать или перестраиваться. | H1, H10 |
| 5 | Уровни `minimal/operational/reproducible/forensic` | N | Уровни становятся operator-facing presets для H10 capture modes и retention, но не меняют состав канонических H1/H2/H6/H8 событий и audit facts. | H10; amendment A |
| 6 | По умолчанию hashes/metadata, полный контекст только по настройке и после очистки | A | H6 ContextSnapshot хранит exact references, hashes, token accounting, omissions и policy; capture bytes опциональны, purgeable и проходят redaction/secret scanning. H10 capture policy ограничивает режим. | H6, H8, H10 |
| 7 | Resume/checkpoint/replay/reconciliation выбираются по типу шага | A | H1 logical replay не выполняет side effects; H4/H5/H8 reconciliate external operations; H10 safe replay использует substitutions или denial. | H1, H4, H5, H8, H10 |
| 8 | Approval зависит от риска, policy и полномочий | A | Base capability задаёт ceiling; H2 policy может deny, prepare-only, require approval или add obligations. Approval не преодолевает explicit deny или hard budget. | H2, H7 |
| 9 | Артефакт — любой адресуемый результат агента | N | Любой результат может быть материализован как governed Artifact revision, но не каждый logical result обязан становиться Artifact. Малые typed outcomes и references могут оставаться в своих агрегатах. | H6 |
| 10 | Нейтральное название `Vestrace State Engine` | A | Название допустимо как внутренний термин без нового binary, crate requirement, API prefix или product branding. | Этот документ |
| 11 | PostgreSQL как backend | A | PostgreSQL является единственным источником истины для v0.1 и Harness. | Foundation, v0.1, H1–H10 |
| 12 | Техническая детализация событий зависит от уровня журналирования | N | Canonical domain events и обязательный audit не зависят от logging preset. Меняться могут только lossy telemetry, optional capture и diagnostic detail. | H1, H10 |
| 13 | После commit событие исправляется только компенсацией | A | Run events, interaction events, audit records и immutable revisions не редактируются; исправления создают новые facts/revisions/compensations. | H1, H6, H7, H10 |
| 14 | UUIDv7 | A | Foundation использует domain newtypes над `Uuid::now_v7()`. | Foundation |
| 15 | `recorded_at` обязателен, `occurred_at` опционален | A | Этот temporal split уже используется для внешних/interaction facts; внутренние logical events обязаны иметь authoritative recorded timestamp. Отсутствие occurred time не блокирует запись. | v0.1 events, H1, H7 |
| 16 | Task ownership плюс optimistic versioning | A | H1 использует `RunVersion`, leases и atomic commit; H5 Coordinator/RunStep assignment определяют ownership. Lease heartbeat не меняет logical version. | H1, H5 |
| 17 | Подагент получает сформированный Coordinator context snapshot | A | H5 delegation использует allowlisted delegated context; H6 создаёт immutable ContextSnapshot с exact source revisions и policy decisions. | H5, H6 |
| 18 | Domain history долговечна; technical logs архивируются/удаляются по policy | A | H6 retention/purge и H10 capture/audit разделяют durable facts, purgeable content и lossy telemetry. | H6, H10 |
| 19 | Секреты не хранятся в runtime DB, только references и usage facts | A | H8 хранит opaque SecretReference, credential generations, leases и content-free audit; secret bytes находятся только за `SecretBackendPort`. | H8 |
| 20 | Artifact адресуется URI, media type и content hash | N | Каноническая identity — typed `ArtifactId` + immutable `ArtifactRevisionId`; content hash идентифицирует bytes, но не permission. Читаемый URI/alias допустим только как presentation/adapter mapping. | H6, H11 |
| 21 | Новый набор Run statuses с `degraded/recovery_required/outcome_unknown/completed` | C | Сохраняется утверждённый H1 набор: `Created`, `Preparing`, `Running`, durable waiting states, `Paused`, `PausedPolicyChanged`, `Succeeded`, `SucceededWithWarnings`, `Partial`, `Failed`, `Cancelled`, `Expired`. `Unknown` принадлежит RunStep/invocation, а не общему Run status. | H1, Harness v0.2 |
| 22 | Отдельная расширяемая модель Task statuses | C | Harness является Run-first. Исполнение представлено immutable plan revisions и `RunStep`; новый mutable Task aggregate дублировал бы H1/H5. `Task` остаётся допустимым memory kind, но не execution root. | v0.1 memory, H1, H5 |
| 23 | Completion criteria плюс независимая verification для рискованных задач | A | H5 хранит explicit success criteria; H10 risk-oriented verification и completion gate обязательны перед `Succeeded`. Deterministic evidence имеет приоритет над model judge. | H5, H10 |
| 24 | Tool idempotency declaration, idempotency key и retry policy | A | H4 Tool revision/operation определяет side-effect class, idempotency, retry, cancellation и reconciliation; H8 connector operation объявляет поддержку idempotency/reconciliation. | H4, H8 |
| 25 | Неизвестный исход внешнего действия переводится в Unknown и reconciliate | A | Ambiguous completion никогда не считается failure или permission to retry; reconciliation выполняется до correction/retry. | H4, H5, H8, H10 |
| 26 | Checkpoint содержит Run/step state, context, artifacts и незавершённые действия | A | H1 checkpoint содержит authoritative Run snapshot и stable cross-Horizon references; H2–H8 добавляют exact policy, budget, context, artifact и invocation references без копирования secret bytes. | H1, H2, H5, H6, H8 |
| 27 | Checkpoints создаются по policy, boundaries и risk | A | H1 предоставляет durable checkpoints; H2 obligations и H4/H5 execution policies могут требовать checkpoint до guarded operations; frequency остаётся bounded policy. | H1, H2, H4, H5 |
| 28 | Подагент делегирует только при явном праве и depth limit; default 1 | A | H5 фиксирует effective delegated authority как intersection и default depth one. | H5 |
| 29 | Бюджеты: steps, time, tokens, cost, tools, artifacts, subruns и ресурсы | A | H2 использует typed hierarchical dimensions, reservations, allocations, quotas и accounting journals. | H2 |
| 30 | Cooperative cancellation, гарантия зависит от adapter | A | Run cancellation создаёт durable intent; model/tool/remote adapters объявляют cancellation semantics, а ambiguous completion остаётся Unknown. | H1, H3, H4, H5 |
| 31 | Иерархия `Run → Task → Step → Action` | C | Сохраняется `AgentRun → ExecutionPlanRevision → RunStep → typed invocation reference`. Generic Task/Action layer запрещён, пока не доказана недостающая семантика. | H1, H3, H4, H5 |
| 32 | Один Action содержит несколько Attempt | N | Retry history сохраняется владельцем конкретной операции. `RunStep.attempt` отражает scheduling attempt; ModelExecution, ToolInvocation, RemoteAgentInvocation и VerificationAttempt хранят собственные typed attempts. | H1, H3, H4, H5, H10 |
| 33 | Расширяемый registry универсальных Action types | C | H5 использует bounded `PlanStepKind` и typed action payloads. Extensions добавляются через approved component/package/tool/remote contracts, а не через произвольный runtime action registry. | H5, H9 |
| 34 | Расширяемый typed task graph | N | Graph принадлежит immutable ExecutionPlanRevision. `depends_on` является plan dependency; validation, compensation, handoff и evidence остаются отдельными typed contracts, а не произвольными edges mutable Task graph. | H5, H10 |
| 35 | Критичные projections синхронны, аналитические асинхронны | A | H1 aggregate state/event/work commit атомарны; H10 telemetry/read models являются asynchronous/lossy projections и не управляют Run transitions. | H1, H10 |
| 36 | Projection failure создаёт lag/rebuild, поведение зависит от criticality | A | Authoritative commit не зависит от optional telemetry exporter; mandatory transactional audit/outbox следует source subsystem policy. Operational projections имеют cursor, lag и rebuild lifecycle. | H10 |
| 37 | Event schema registry, schema version и upcasters | E | Требуется единая compatibility policy: static versioned event envelopes, generated schemas и read-time upcasters. Mutable runtime schema registry не вводится. | H1, H7, H11; amendment B |
| 38 | Shared memory mount read-only by default, write отдельным разрешением | E | v0.1 запрещает cross-workspace memory. H6 mounts относятся к Artifact revisions и не решают memory sharing. Требуется отдельный `MemoryShareGrant`/`MemoryMount` contract; read-only является default, source write — отдельной protected operation. | v0.1 memory, H6 boundary; amendment F |
| 39 | Memory types включают fact/preference/procedure/summary/decision/lesson/failure_pattern/tool_knowledge | N | Сохраняется утверждённый v0.1 набор `Fact`, `Preference`, `Constraint`, `Decision`, `Task`, `Procedure`, `Observation`, `Outcome`, `Summary`. `lesson`, `failure_pattern` и `tool_knowledge` кодируются structured payload/tags на существующих kinds до появления доказанной retrieval-причины для нового top-level kind. | v0.1 memory, H6 |
| 40 | Агент предлагает memory; policy/Coordinator подтверждает; sensitive/global может требовать user | A | H6 consolidation создаёт governed candidate memories через существующую write policy и никогда не auto-activates их в обход policy. | v0.1 memory policy, H6 |
| 41 | Roles как templates, capabilities как фактическая authority | A | v0.1 security задаёт capabilities; H2 принимает их как непреодолимый base ceiling. | v0.1 security, H2 |
| 42 | Capability ограничивается operation/resource/time/budget/risk/conditions | A | H2 OperationFingerprint, policy snapshot, authorization ticket и budget reservation; H8 credential lease дополнительно связывает destination, generation, expiry и use count. | H2, H8 |
| 43 | Подагент получает только суженное подмножество authority | A | Effective delegated authority — intersection parent authority, target eligibility, request, workflow constraints и current policy. | H5 |
| 44 | Risk levels плюс contextual raising | A | H2 использует typed RiskLevel и layered policy. Untrusted model/tool/external content не может понизить risk; H10 verification derives minimum plan from deterministic risk inputs. | H2, H10 |
| 45 | SQL — единственный интерфейс MVP | C | PostgreSQL является infrastructure detail. Canonical boundary — application ports; v0.1 требует HTTP/MCP, Harness — HTTP/MCP/SDK/event stream/AG-UI. Direct agent SQL запрещён. | v0.1 interfaces, H11/H11A |
| 46 | Internal event bus и внешние webhooks | E | Durable public-event/outbox semantics существуют, но H7 явно не реализует arbitrary webhook sources. Webhooks оформляются отдельным extension boundary после H11, disabled by default. | H7, H11; amendment D |
| 47 | Observability: latency, tokens, cost, retries, queue, failures, quality, verification | A | H10 разделяет bounded telemetry, durable security audit, evaluation metrics, verification outcomes и operational read models. | H10 |
| 48 | Signed portable Run archive с events/schemas/artifacts/checkpoints | E | H6 умеет governed Artifact export, H10 — signed audit checkpoints, но единый portable Run bundle не определён. Требуется administrative export format поверх exact references. | H6, H10, H11; amendment C |
| 49 | Logical deletion, cryptographic erasure и separate physical purge | A | H6 различает delete/retention/legal hold/purge/GC; H8 удаляет secret material; H10 сохраняет content-free tombstones/audit explanations. | H6, H8, H10 |
| 50 | Все функции State Engine входят в `v0.1` | C | Версии репозитория сохраняются: Vestrace v0.1 — memory/cognitive asset server; Harness v0.2 — runtime Horizons H1–H11. Reconciliation не перенумеровывает roadmap. | v0.1, Harness v0.2 |
| 51 | Только State Engine выполняет controlled SQL; analytics получает read-only views | N | Запись действительно изолирована repository/application ports. Read-only views допустимы только как H10 operational projections/admin adapters с forced RLS; они не являются agent authority. | Foundation, H1, H10 |
| 52 | Rust implementation | A | Rust Edition 2024 является утверждённым стеком. | Foundation |
| 53 | Local CAS плюс readable aliases/virtual workspace paths | A | H6 определяет secure local content-addressed store, opaque namespace/blob keys и safe logical references; physical paths не видны моделям. | H6 |
| 54 | PostgreSQL FTS плюс pgvector | A | v0.1 retrieval и H6 context используют PostgreSQL FTS/pgvector; embeddings и chunks являются rebuildable projections. | v0.1 retrieval, H6 |
| 55 | Incoming/outgoing signed webhooks, allowlist, risk policy, disabled by default | E | Намерение сохраняется, но выносится из H7 first slice. Inbound webhook создаёт untrusted trigger/interaction fact и никогда не мутирует Run напрямую; outbound delivery использует durable outbox и at-least-once semantics. | Future H7/H11 extension; amendment D |
| 56 | Application-level encryption, separate workspace keys in external secret store | E | H8 уже отделяет secret material, но H6/H10 не фиксируют общий workspace envelope-encryption contract для sensitive blobs/payloads. Требуется отдельный security amendment без шифрования indexed metadata, нужной RLS/search. | H6, H8, H10; amendment E |

## 5. Обязательные уточнения к будущим контрактам

### Amendment A — State capture profiles

Пользовательские профили сохраняются как presets, а не как новый event taxonomy:

| Profile | H10 capture baseline | Допустимое содержание |
|---|---|---|
| `minimal` | `MetadataOnly` | обязательные Run/policy/accounting/audit references, statuses, hashes и health counters; без optional content capture |
| `operational` | `StructuredOnly` | bounded structured diagnostics, usage, latency, retry/failure categories; без raw payloads |
| `reproducible` | `Redacted` + exact references | deterministic inputs, revisions, hashes, omissions, redacted captures и Artifact refs, если policy разрешает |
| `forensic` | `Full` only where explicitly allowed | максимально полный governed capture с quarantine, retention, purge и export controls; никогда hidden reasoning или secrets |

Инварианты:

1. Профиль не отключает обязательный security audit.
2. Профиль не изменяет canonical H1/H2/H6/H8 events.
3. Более высокий профиль не преодолевает sensitivity, provider-transfer, secret или retention policy.
4. Capture bytes являются H6 Artifacts и могут быть cryptographically erased/purged.

### Amendment B — Event schema evolution

Для durable/public событий требуется единая статическая compatibility policy:

```text
EventEnvelope
├── event_id
├── stable event kind
├── schema_version
├── aggregate/reference IDs
├── sequence/cursor
├── occurred_at?
├── recorded_at
├── payload
└── payload_hash
```

Правила:

1. Schema version начинается с `1` для каждого stable event kind.
2. Значение существующего event kind не меняется семантически.
3. Breaking payload change создаёт новую schema version либо новый event kind.
4. Generated JSON Schemas хранятся в versioned repository paths.
5. Upcaster является pure read-time function и не переписывает journal.
6. Unsupported future schema fail closed с stable compatibility error.
7. External event/public DTO compatibility и internal Rust enum compatibility проверяются отдельными tests.
8. Runtime-mutable schema registry, позволяющий модели регистрировать новый event kind, запрещён.

### Amendment C — Signed Run export

Будущий administrative export должен быть exact-reference bundle, а не PostgreSQL dump:

```text
vestrace-run-export/
├── manifest.json
├── run.json
├── events.ndjson
├── plan-revisions/
├── checkpoints/
├── policy-and-budget-references/
├── context-manifests/
├── artifact-manifest.json
├── artifacts/                 # optional exact revisions
├── audit-chain-proof/
├── schemas/
├── checksums.sha256
└── signature.json
```

Инварианты:

- export проходит H2 authorization и H6 classification/export policy;
- secret bytes, credential leases, backend paths и hidden reasoning не экспортируются;
- missing/purged content описывается tombstone, а не реконструируется;
- import сначала проверяет format version, hashes, signature и workspace binding;
- import не активирует Run, trigger, connection, credential, memory или external work автоматически;
- replay imported bundle остаётся side-effect-free.

### Amendment D — Webhook boundary

Webhooks не добавляются внутрь H7 без отдельного design/plan.

Inbound contract:

```text
HTTP request
→ origin/allowlist check
→ signature and timestamp verification
→ replay-window/dedup check
→ bounded payload capture/quarantine
→ authenticated or untrusted source fact
→ H7 trigger evaluation
→ optional RunProposal/Run under H2 policy
```

Outbound contract:

```text
canonical source event
→ transactional delivery intent
→ destination policy and classification check
→ signed bounded payload
→ at-least-once delivery
→ receipt/failure/retry audit
```

Обязательные свойства:

- disabled by default;
- exact endpoint revision и destination allowlist;
- no direct Run mutation;
- no secret-bearing arbitrary headers;
- one-way operation fingerprint and idempotency key;
- timestamped signature and replay protection;
- retry policy must not duplicate an external commitment without receiver idempotency support;
- payload schema and maximum bytes are versioned;
- outgoing capture cannot exceed destination classification policy.

### Amendment E — Workspace envelope encryption

Требуется отдельный H6/H8 security amendment со следующей моделью:

```text
workspace KEK reference       -> H8 SecretBackendPort
per-object/per-generation DEK -> wrapped by workspace KEK
encrypted content             -> H6 ArtifactBlobStore or bounded encrypted side payload
searchable metadata           -> PostgreSQL, classified and minimized
```

Шифровать на уровне приложения следует:

- optional context-capture bytes;
- sensitive Artifact/representation blobs;
- sensitive diagnostic captures;
- bounded side payloads, которые не должны индексироваться;
- portable export content перед упаковкой, когда export policy этого требует.

Не следует непрозрачно шифровать:

- identifiers и revisions;
- workspace/RLS ownership columns;
- lifecycle statuses;
- operation fingerprints и content-free audit facts;
- поля, необходимые для deterministic uniqueness/concurrency;
- FTS/vector source без отдельной redacted/search projection strategy.

Ключевые правила:

1. KEK bytes не входят в PostgreSQL, events, work items или model context.
2. Rotation создаёт новую key generation и не меняет historical hashes незаметно.
3. Cryptographic erasure уничтожает wrapped DEK/KEK generation и оставляет content-free tombstone.
4. Cross-workspace dedup остаётся disabled by default.
5. Backup/restore документирует key dependencies и не обещает восстановление без key material.
6. Search projection не может раскрывать исходный sensitive content после erasure.

### Amendment F — Cross-workspace memory sharing

v0.1 сохраняет строгий запрет межпространственной памяти. Будущее расширение должно быть отдельной protected capability, а не новым значением `MemoryScope`.

Минимальная модель:

```text
MemoryShareGrantRevision
├── source_workspace_id
├── target_workspace_id
├── allowed memory IDs/revisions or bounded source scope
├── allowed kinds and labels
├── maximum sensitivity
├── valid_from / valid_until
├── policy decision and approver
└── immutable content hash

MemoryMount
├── target_workspace_id
├── share_grant_revision_id
├── status
├── read policy
└── retrieval projection cursor
```

Инварианты:

1. Read-only является default и не выдаёт source-workspace write capability.
2. Target retrieval сохраняет source workspace, exact memory revision и provenance; shared result не выглядит локальной памятью.
3. Знание memory ID, embedding или content hash не даёт доступа.
4. Query выполняется через source-authorized application port либо policy-filtered projection; прямой cross-workspace SQL/RLS bypass запрещён.
5. Изменение target copy создаёт новую локальную Memory с derivation reference, а не изменяет source.
6. Изменение source требует отдельной operation-bound capability/approval и не следует из mount.
7. Revocation немедленно блокирует новые reads/context assembly; уже созданные captures и derived local memories следуют собственной provenance/retention policy.
8. Shared embeddings, search documents и summaries не становятся глобальными authoritative records и должны удаляться при revoke/purge.
9. `Restricted`/`Secret` memory не монтируется без explicit source policy, target policy и user approval.
10. Cross-workspace physical dedup остаётся disabled by default.

## 6. Нормативные последствия

После утверждения этого reconciliation design должны быть подготовлены отдельные documentation-only изменения:

1. amendment к Harness v0.2 design, добавляющий внутренний термин State Engine и ссылку на этот документ;
2. amendment/ADR для capture-profile presets;
3. compatibility ADR для event schema versioning/upcasters;
4. design + implementation plan для signed Run export;
5. design + implementation plan для webhook extension после стабилизации H11;
6. security design + implementation plan для workspace envelope encryption;
7. memory-sharing design + implementation plan без изменения v0.1 workspace boundary;
8. cross-plan wording correction, запрещающая параллельные generic `Task/Action/Attempt` aggregates и direct SQL boundary.

Эти документы должны ссылаться на существующие Horizons и не создавать `H12 State Engine` как новый параллельный runtime.

## 7. Проверка непротиворечивости

Документ считается согласованным только при выполнении следующих условий:

- H1 остаётся единственным владельцем `AgentRun`, `RunStep`, `RunEvent`, checkpoint, lease и logical replay;
- H2 остаётся единственным владельцем final action authorization, approvals и budgets;
- H6 остаётся единственным владельцем Artifact/ContextSnapshot bytes, provenance, Artifact mounts, retention и consolidation;
- v0.1 memory boundary остаётся workspace-local до отдельного утверждённого memory-sharing amendment;
- H8 остаётся единственным владельцем secret material и credential leases;
- H10 telemetry/projections не управляют authoritative state;
- H11 public APIs не предоставляют arbitrary SQL или storage credentials;
- ни один amendment не сохраняет hidden chain-of-thought;
- `Unknown` никогда не означает permission to retry;
- hash/ID/reference никогда не означает read permission;
- export/import/replay никогда не запускают external side effects автоматически.

## 8. Финальное решение

Из 56 решений большинство сохраняется без изменения или после терминологической нормализации. Отдельная State Engine СУБД и отдельная execution hierarchy не создаются.

Нормативный результат:

> Vestrace State Engine — не новый слой рядом с Vestrace, а согласованное имя для уже распределённой durable-state ответственности Harness. Его ядром остаются H1 atomic Run journal и checkpoints; authority принадлежит H2; typed execution — H3–H5; context/artifacts/local memory consolidation — H6; interactions/triggers — H7; secrets — H8; audit/evaluation/projections — H10; public access — H11. Cross-workspace memory остаётся отдельным будущим extension boundary.
