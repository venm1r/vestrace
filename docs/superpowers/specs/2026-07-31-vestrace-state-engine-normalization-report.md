# Vestrace State Engine — cross-plan normalization report

**Статус:** предложено для письменного утверждения  
**Дата аудита:** 2026-08-01  
**Репозиторий:** `venm1r/vestrace`  
**Ветка аудита:** `agent/state-engine-normalization-audit`  
**Характер изменения:** documentation-only; без Rust-кода, миграций, generated schemas, CI и runtime configuration

> Этот отчёт не создаёт новый Horizon, runtime, event store, aggregate или implementation scope. Он фиксирует нормативное чтение уже утверждённых документов и устраняет неоднозначность владения между существующими планами.

## 1. Итог

Cross-plan audit завершён со следующим результатом:

```text
unresolved behavioral conflicts = 0
new runtime authorities         = 0
new Horizons                    = 0
existing source documents edited = 0
normalization report added      = 1
```

Ни один из рассмотренных документов не требует изменения поведения уже утверждённого design.

Обнаруженные неоднозначности относятся к терминологии и размещению будущего implementation-кода, а не к конкурирующим authority contracts. Они разрешены настоящим отчётом через явный ownership/precedence map.

## 2. Нормативный результат

После нормализации Vestrace имеет:

- одного владельца durable Run state — H1;
- одного владельца окончательного authorization/approval/budget решения — H2;
- один authoritative Memory Core — v0.1 Memory Plane;
- одного владельца Context/Artifact bytes и lifecycle — H6;
- одного владельца interaction/trigger/public-event semantics — H7;
- одного владельца credential и cryptographic-operation authority — H8;
- одного владельца audit/evaluation/replay-for-evaluation semantics — H10;
- один product/API/SDK/UI integration layer — H11;
- отсутствие H12 State Engine;
- отсутствие второго event store;
- отсутствие generic mutable `Task/Action/Attempt` aggregate hierarchy;
- отсутствие agent-facing direct SQL;
- отсутствие автоматического повторения неоднозначных внешних side effects;
- отсутствие неявного cross-workspace memory access.

## 3. Утверждённые документы и evidence

| Concern | Нормативный документ | Merge commit |
|---|---|---|
| State Engine reconciliation | `docs/superpowers/specs/2026-07-31-vestrace-state-engine-reconciliation-design.md` | `26d2cd895e110722b20883c5e29315676b86480c` |
| Follow-up documentation program | `docs/superpowers/plans/2026-07-31-vestrace-state-engine-followup-documentation.md` | `692012f10135b65e488f924ef3ad61ba566db194` |
| State Engine boundary | `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md` | `8ca111b6273b3c1f9a613b0730df94329ec11db4` |
| Capture profiles | `docs/superpowers/specs/2026-07-31-vestrace-state-capture-profiles-adr.md` | `24f02ed362b2af0361fcc9d499d275e21257c0e2` |
| Durable event compatibility | `docs/superpowers/specs/2026-07-31-vestrace-event-schema-compatibility-adr.md` | `3f7a196dfc879f628628b31d9eddb9b18770de10` |
| Signed portable Run export | `docs/superpowers/specs/2026-08-01-vestrace-signed-run-export-design.md` | `c27e76fda273c6aa82e622f0b47f32e4de69d78a` |
| Webhook extension | `docs/superpowers/specs/2026-08-01-vestrace-webhook-extension-design.md` | `8dfb8c11e8ff2ec9fba5ef95aa158d49d1b27f50` |
| Workspace envelope encryption | `docs/superpowers/specs/2026-08-01-vestrace-workspace-envelope-encryption-design.md` | `9b3ad5c274907f0be7bcf4e195ab0b2858e2e999` |
| Cross-workspace memory sharing | `docs/superpowers/specs/2026-08-01-vestrace-cross-workspace-memory-sharing-design.md` | `5961d0d65c16bc5d067aeaa815d42f00becceaff` |

Merged design documents outrank older planning shorthand when they discuss the same exact concern. They do not replace the owning Horizon; they constrain how that Horizon may be implemented.

## 4. Классификация

Для каждого finding используется ровно одна из категорий:

```text
consistent
terminology-only correction
behavioral conflict
intentional historical context
owned by future design
```

Интерпретация:

- `consistent` — документы выражают совместимое поведение и ownership;
- `terminology-only correction` — поведение совместимо, но формулировку необходимо читать через нормализованное определение;
- `behavioral conflict` — два документа требуют несовместимого runtime-поведения;
- `intentional historical context` — документ описывает отвергнутый или прежний вариант только для объяснения решения;
- `owned by future design` — базовый документ намеренно не определяет extension, а отдельный approved design владеет расширением.

## 5. Нормативная ownership matrix

| Область | Единственный authoritative owner | Остальные роли |
|---|---|---|
| `AgentRun`, `RunStep`, `RunEvent`, Run version, checkpoints, logical replay, Run work items | H1 | H3–H11 вызывают H1 ports и добавляют typed owned records |
| Capabilities, contextual policy, approvals, tickets, budgets, quotas | H2 | остальные Horizons являются enforcement points или consumers |
| Model routing, model executions и attempts | H3 | H10 оценивает, H11 отображает |
| Tool/Sandbox execution и external Tool effects | H4 | H2 разрешает, H10 проверяет |
| Plans, delegation, SubRuns и handoffs | H5 | H1 хранит Run links; H10 оценивает |
| Memory identity, revisions, provenance, conflicts, lifecycle и write policy | v0.1 Memory Core | H6 собирает context и proposes candidates; sharing является отдельной boundary |
| Context snapshots, Artifact identity/revisions, blob lifecycle, evidence, export и purge | H6 | H8 выполняет key operations; H10 использует references |
| Conversations, interactions, human continuation, triggers, source facts, public events | H7 | H11 предоставляет transports и UI |
| Connections, secret references, key/signing operations, credential leases | H8 | H2 разрешает use; adapters потребляют operation-bound leases |
| Packages, profiles и extensions | H9 | H10 даёт gate evidence; H11 упаковывает release |
| A2A wire mapping и remote interoperability gateway | H9A | H5 владеет delegation semantics; H11 монтирует surface |
| Telemetry projections, security audit, evaluation, verification, safe replay и explanations | H10 | source Horizons остаются authoritative |
| HTTP/CLI/MCP/SDK/UI, release, deployment, backup/restore product surface | H11 | не создаёт parallel domain authorities |
| State Engine | composite internal boundary H1–H11 | не является Horizon, service, database или public API |

## 6. Findings

### N-01 — термин `State Engine`

**Классификация:** `terminology-only correction`

Нормативное чтение:

`Vestrace State Engine` — внутреннее имя совокупной durable-state boundary Harness. Это не отдельный binary, crate, PostgreSQL schema owner, API, event store, worker или Horizon.

Любое будущее использование термина обязано указывать owning subsystem: H1, H2, H3, H4, H5, Memory Core, H6, H7, H8, H9, H9A, H10 или H11.

**Разрешение:** boundary amendment является authoritative definition.

### N-02 — второй event store

**Классификация:** `consistent`

H1 хранит ordered Run journal. v0.1 Memory Core хранит собственные memory/domain events. H7 хранит interaction/public-event records. H10 хранит audit/evaluation records.

Это разные typed journals с разными owners, а не конкурирующие глобальные event stores.

Event compatibility ADR задаёт общий serialization discipline, но не создаёт отдельное хранилище.

### N-03 — generic `Run → Task → Step → Action → Attempt`

**Классификация:** `intentional historical context`

Первоначальная State Engine модель рассматривала generic hierarchy. Reconciliation и boundary amendment отвергли её как parallel aggregate model.

Нормативная модель:

- H1: Run/Step;
- H3: ModelExecution/Attempt;
- H4: ToolInvocation/Attempt;
- H5: Plan/SubRun/Handoff;
- H7: TriggerOccurrence/HumanContinuation;
- H8: CredentialLease/Use;
- H10: Evaluation/Verification/Replay records;
- H11: transfer/delivery/backup operations.

Typed subsystem records не должны быть обёрнуты в generic mutable `Action`/`Attempt` authority.

### N-04 — Run status vocabulary

**Классификация:** `terminology-only correction`

State Engine discussions использовали conceptual labels `recovery_required` и `outcome_unknown`. Они не требуют добавления универсальных H1 statuses.

Нормативное правило:

- H1 использует утверждённый Run status contract;
- owning subsystem хранит typed `Unknown`, `completion_unknown`, reconciliation или degraded records;
- H1 Run может ждать dependency/reconciliation через существующий durable state;
- публичный surface может отображать normalized explanation, но не создавать новый authoritative status без отдельного H1 amendment.

### N-05 — logical replay, safe replay и Run export

**Классификация:** `terminology-only correction`

Три механизма различны:

```text
H1 logical replay
= восстановление authoritative Run state из H1 journal/checkpoints

H10 safe replay
= side-effect-free evaluation captured work в изолированном scope

vestrace-run-export import
= inert verification/inspection/staging signed bundle
```

Ни один из них не разрешает автоматическое повторение external commitment.

### N-06 — автоматический retry внешних действий

**Классификация:** `consistent`

H3, H4, H5/H9A, H7 notifications, H8 token exchange, webhooks и encryption key destruction используют typed idempotency/reconciliation rules.

Если completion может быть применён извне, blind retry запрещён до reconciliation или exact idempotency proof.

### N-07 — direct SQL

**Классификация:** `consistent`

PostgreSQL является authoritative persistence backend, но SQL не является agent-facing mutation API.

Допустимо:

- controlled repositories;
- prepared queries;
- scoped transactions;
- forced RLS;
- bounded read-only operational/analytical projections.

Недопустимо:

- arbitrary model-generated SQL;
- прямое изменение projection без owning command/event transaction;
- cross-workspace joins, обходящие sharing boundary;
- public unrestricted state PATCH/SQL interface.

### N-08 — capture profiles и H10 capture modes

**Классификация:** `consistent`

Profiles являются operator presets поверх существующего H10 lattice:

```text
Disabled < MetadataOnly < StructuredOnly < Redacted < Full
```

Mapping:

```text
Minimal       → MetadataOnly baseline
Operational   → StructuredOnly baseline
Reproducible  → Redacted baseline
Forensic      → Full baseline только при explicit policy permit
```

Profile не изменяет canonical event emission, mandatory audit или authority.

### N-09 — logging level меняет domain events

**Классификация:** `intentional historical context`

Раннее решение допускало зависимость event boundary от logging level. Capture ADR нормализовал это:

- canonical domain events стабильны;
- optional capture richness меняется;
- отсутствие payload приводит к explicit omission/NotReplayable/Inconclusive;
- событие не исчезает из authoritative journal из-за operator profile.

### N-10 — event schema registry

**Классификация:** `consistent`

Event compatibility ADR определяет:

```text
stable_event_kind + per-kind schema_version
```

Ownership:

- source Horizon владеет event meaning;
- repository-owned registry владеет schemas/upcasters;
- H11 публикует external schemas;
- H7 владеет public-event transport semantics;
- H10 проверяет audit/evaluation compatibility;
- models/packages/extensions/webhooks не регистрируют canonical kinds.

Registry не является mutable runtime plugin registry.

### N-11 — historical journal rewriting

**Классификация:** `consistent`

Старые event rows не переписываются. Compatibility обеспечивается pure deterministic read-time upcasters.

Unknown schema version fail-closed для semantic replay/resume/reconciliation/import.

### N-12 — H6 Artifact definition

**Классификация:** `terminology-only correction`

Не каждый addressable result автоматически является H6 Artifact.

Нормативно:

- candidate/result/reference может существовать в owning subsystem;
- H6 Artifact появляется только после H6 materialization, classification, quarantine/inspection и immutable revision finalization;
- hash, URI, store path или external receipt не создают Artifact authority.

### N-13 — H6 Artifact mounts и MemoryMount

**Классификация:** `terminology-only correction`

Это разные contracts:

- H6 Artifact mount — short-lived exact-revision access к Artifact bytes;
- MemoryMount — target acceptance exact MemoryShareGrantRevision для federated read.

Они не взаимозаменяемы, не наследуют права друг друга и не используют общий generic mount aggregate.

### N-14 — signed Run export и H11 backup/restore

**Классификация:** `consistent`

`vestrace-run-export`:

- один Run и selected related evidence;
- inert inspection/evaluation/migration staging;
- не содержит credentials/keys;
- не запускает Run;
- не восстанавливает deployment.

H11 backup/restore:

- disaster recovery deployment identity;
- отдельный encrypted BackupStore;
- recovery validation;
- restore fencing;
- не является cross-deployment clone.

Один формат не заменяет другой.

### N-15 — signature semantics

**Классификация:** `consistent`

Подпись подтверждает integrity exact bytes/manifest под известным trust anchor. Она не является capability, approval, authorization ticket или доказательством semantic truth.

Правило одинаково применяется к audit checkpoints, Run export, release manifests, backups и webhook payloads.

### N-16 — webhook ownership H7/H11

**Классификация:** `terminology-only correction`

Нормативное разделение:

- H7 владеет source facts, trigger occurrences/evaluation, public source events и causal/storm semantics;
- H2 владеет authorization/risk/approval;
- H8 владеет signing/credential operations;
- H10 владеет audit/telemetry;
- H11 владеет management, inspection и transport adapters;
- H1 владеет Run state, который webhook не изменяет напрямую.

H11 paths `domain/webhook` и `application/webhook` в старом implementation plan следует читать как предварительное размещение product-surface delivery contracts, а не как право H11 создать parallel trigger/public-event authority.

Будущий webhook implementation plan обязан выбрать одно точное module placement без дублирования H7 records.

### N-17 — inbound webhook

**Классификация:** `owned by future design`

H7 первоначально оставлял `ExternalEvent` inactive без compatible extension. Approved webhook design является таким extension contract.

До отдельного implementation plan и explicit окончания documentation-only phase:

- endpoints не существуют;
- feature disabled;
- source facts не принимаются;
- trigger contract не активирован.

### N-18 — outbound webhook и H11 existing plan

**Классификация:** `consistent`

Оба документа требуют:

- at-least-once delivery;
- immutable source event/body identity;
- receiver deduplication;
- H8-backed signing;
- no direct Run mutation;
- SSRF/redirect controls;
- durable intent/attempt state;
- `OutcomeUnknown` вместо unsafe retry.

Webhook extension design является более точным semantic contract и применяется при будущем implementation planning.

### N-19 — webhooks disabled by default

**Классификация:** `terminology-only correction`

Наличие webhook code, release fixture или management surface в H11 не означает activation.

Нормативный default:

```text
Deployment webhook capability = Disabled
Workspace webhook capability  = Disabled
```

Reference fixtures могут тестировать явно включённый isolated profile, но release/install не включает endpoints автоматически.

### N-20 — H8 secret encryption и workspace payload encryption

**Классификация:** `consistent`

H8 initial local secret backend и workspace envelope encryption имеют разные scopes:

- H8 secret backend защищает credential/key material;
- workspace envelope encryption защищает selected H6/memory/capture/webhook/evaluation payloads;
- H8 KEK operation boundary обслуживает wrap/unwrap/sign/destroy;
- payload ciphertext остаётся в H6/bounded payload storage.

Разные initial algorithm profiles не являются конфликтом, поскольку это разные cryptographic domains и formats.

### N-21 — plaintext fallback

**Классификация:** `consistent`

Если H8 key backend недоступен:

- sensitive required write fail-closed;
- optional capture может быть понижен до no-payload mode;
- plaintext не сохраняется как fallback;
- audit фиксирует content-free failure.

### N-22 — KEK rotation, DEK rotation и algorithm migration

**Классификация:** `terminology-only correction`

Операции различны:

```text
KEK rotation     = rewrap существующего DEK
DEK rotation     = новая encrypted object generation
algorithm change = новая encrypted object generation
```

Ни одна из них не переписывает authoritative history молча.

### N-23 — cryptographic erasure и hard purge

**Классификация:** `consistent`

Cryptographic erasure и physical deletion являются отдельными lifecycle operations.

Erasure scope включает wrapped DEK/KEK copies, backup/escrow truth, FTS, pgvector, excerpts, caches и derived payloads.

Unknown destroy result остаётся `OutcomeUnknown`; success не предполагается.

### N-24 — v0.1 `global` memory scope

**Классификация:** `consistent`

`global` продолжает означать только workspace-global. Approved sharing design не расширяет scope существующей Memory.

Cross-workspace access существует только через exact grant + exact target acceptance.

### N-25 — cross-workspace memory support

**Классификация:** `owned by future design`

v0.1 корректно запрещает cross-workspace references. Sharing design добавляет отдельную extension boundary:

```text
source MemoryShareGrant
→ immutable GrantRevision
→ target MemoryMount acceptance
→ federated policy-checked read
```

Он не изменяет Memory Core foreign-key/RLS invariant.

### N-26 — federated retrieval и RLS

**Классификация:** `consistent`

Federated retrieval выполняет отдельные source-authorized queries под source workspace context. Затем результаты проходят target policy intersection и bounded reranking.

Raw cross-workspace SQL join и отключение RLS запрещены.

### N-27 — mounted memory authority

**Классификация:** `consistent`

Mounted view:

- остаётся foreign/untrusted;
- не является local Memory;
- не получает local confidence;
- не supersede local knowledge;
- не создаёт embedding/copy по умолчанию;
- не может быть reshared;
- не может быть использован моделью, экспортирован или импортирован без отдельных permissions.

### N-28 — local derivation/import from shared memory

**Классификация:** `consistent`

Локальная запись требует `MemoryImportProposal`, source authorization, target policy и обычный Memory Core lifecycle.

Persistent target representation получает fresh target DEK и полное provenance. Source Memory не изменяется.

### N-29 — revoke/purge и active Runs

**Классификация:** `consistent`

Revoked/stale/expired mount блокирует future use. Context caches/indexes quarantine/invalidate.

Уже журналированные historical facts не переписываются, но новый external/model invocation со stale foreign context запрещён без нового context snapshot и authorization.

### N-30 — H10 telemetry как source of truth

**Классификация:** `consistent`

Operational telemetry является lossy projection. Она не меняет Run, budget, policy, billing или completion truth.

Durable audit/evaluation aggregates не заменяют source subsystem records.

### N-31 — synchronous critical projection и asynchronous analytics

**Классификация:** `consistent`

Critical state принадлежит owning transaction/repository. H10 analytics/read models асинхронны и могут lag/rebuild.

Projection failure не разрешает reconstructed mutation и не превращает dashboard в authority.

### N-32 — completion verification

**Классификация:** `consistent`

H1 владеет terminal transition, а H10 выдаёт one-use `VerifiedRunCompletion` evidence для exact Run version/result hash.

Это enforcement dependency, а не второй Run owner.

### N-33 — roles и capabilities

**Классификация:** `consistent`

Role/profile/template определяет expected behavior. Фактическая authority определяется H2 capability/policy/ticket и operation-bound constraints.

Agent snapshot, role name, package, mount, webhook signature, hash или URI не являются permission.

### N-34 — delegation rights

**Классификация:** `consistent`

SubRun/remote agent получает explicit narrowed grants. Права не наследуются автоматически и не могут превышать parent/source ceiling.

Connections и shared memory требуют отдельного delegated grant/permission.

### N-35 — H11 как integration layer

**Классификация:** `consistent`

H11 может добавлять durable product operations — transfer, webhook delivery, backup, restore, upgrade — но не дублирует H1–H10 aggregates.

Public DTO, SDK и console state не являются domain state.

### N-36 — H11 schema publication

**Классификация:** `consistent`

H11 публикует versioned JSON Schema/OpenAPI/SDK contracts. Source schema meaning остаётся у owning Horizon и event compatibility registry.

Publication не предоставляет runtime schema-registration authority extensions/models.

### N-37 — hidden reasoning

**Классификация:** `consistent`

Hidden chain-of-thought/provider private reasoning не является обязательным persisted content и запрещён во всех capture/export/sharing/encryption/webhook paths.

Encryption не делает запрещённое содержимое допустимым.

### N-38 — secrets in events/artifacts/exports

**Классификация:** `consistent`

Secret values остаются только за H8 SecretBackendPort. Domain records используют opaque references и content-free facts.

Run export, memory sharing и webhook payloads не переносят raw secrets или usable source key references.

### N-39 — signed Run export cross-workspace import

**Классификация:** `consistent`

Source IDs сохраняются как provenance. Target IDs/authority не создаются автоматически.

Cross-workspace/deployment import требует explicit remap and policy; импорт остаётся inert.

### N-40 — migration ownership

**Классификация:** `consistent`

Новые designs не получают migration numbers на design phase. Номера выделяются только отдельными implementation plans после проверки всех существующих ranges.

Ни один applied migration не редактируется.

## 7. Проверка обязательных invariants

### 7.1 Один Run owner

**PASS:** H1 является единственным owner `AgentRun`, `RunStep`, `RunEvent`, Run version, checkpoints и logical replay.

### 7.2 Один final authorization owner

**PASS:** H2 является final contextual authorization/approval/budget owner; base capability deny остаётся final.

### 7.3 Один Artifact/Context byte owner

**PASS:** H6 владеет Artifact/Context bytes, classifications, quarantine, export, retention и purge.

### 7.4 Один credential/key-operation owner

**PASS:** H8 владеет secret bytes, key references, signing/wrap/unwrap/destroy operations и credential leases.

### 7.5 Один audit/evaluation owner

**PASS:** H10 владеет audit integrity, evaluation, verification, regression evidence, safe replay и explanations.

### 7.6 Нет direct agent SQL

**PASS:** agents/models используют typed application ports; PostgreSQL доступен через scoped repositories/transactions.

### 7.7 Нет H12 State Engine

**PASS:** State Engine остаётся composite internal boundary.

### 7.8 Нет automatic side-effect replay

**PASS:** unknown/ambiguous operations требуют reconciliation или exact idempotency proof.

### 7.9 Нет implicit cross-workspace sharing

**PASS:** v0.1 isolation остаётся default; sharing требует exact grant/revision/mount acceptance.

### 7.10 Нет второго schema authority

**PASS:** repository-owned event compatibility registry и source Horizon meaning; H11 только publishes.

## 8. Source document modifications

В этом audit PR существующие документы не изменяются.

Причина:

1. подтверждённых несовместимых runtime requirements не обнаружено;
2. старые формулировки не требуют удаления — они либо уже ограничены approved design, либо являются non-authoritative implementation placement;
3. массовая вставка ссылок в H1–H11 увеличила бы diff без изменения поведения;
4. отдельные implementation plans из Task 9 обязаны напрямую импортировать утверждённые contracts и тем самым не повторять старую неоднозначность.

Настоящий отчёт является единым normalization/precedence index для подготовки этих implementation plans.

## 9. Requirements для будущих implementation plans

Каждый plan обязан:

- указать owning Horizon и не создавать H12;
- ссылаться на merged design path и commit;
- использовать typed existing aggregates, а не generic Task/Action/Attempt;
- определить exact files, ports и migrations только своего concern;
- написать failing tests до implementation steps;
- включить restart/idempotency/reconciliation tests;
- включить workspace/RLS/capability boundary tests;
- не разрешать implementation самим фактом merge документа;
- остановиться перед production-code phase до explicit user instruction.

Дополнительные обязательные mappings:

| Implementation plan | Обязательные owners/contracts |
|---|---|
| Capture profiles | H10 capture lattice + H2/H6 policy/export gates |
| Event compatibility | source Horizons + H1/H7/H10/H11 boundaries |
| Signed Run export | H1 snapshot + H2 auth + H6 bytes + H8 signing + H10 audit + H11 adapters |
| Webhooks | H7 source/public events + H2 auth + H8 signing + H10 audit + H11 management/transports |
| Workspace encryption | H6 object lifecycle + H8 key operations + H2 admin authorization + H10 content-free audit |
| Memory sharing | v0.1 Memory Core + H2 + H6 + H8 + H10 + H11 |

## 10. Запрещённые reintroductions

Будущие планы и код не должны повторно вводить:

- `H12 State Engine`;
- global generic event store как новый owner всех domains;
- mutable generic `Task`, `Action` или `Attempt` aggregate поверх typed subsystems;
- agent/model direct SQL;
- event suppression в зависимости от capture profile;
- historical journal rewriting;
- implicit side-effect retry;
- export/import, который запускает Run;
- webhook, который напрямую меняет Run/approval/memory;
- plaintext fallback при key outage;
- wildcard cross-workspace memory grant;
- mount-of-mount;
- source KEK/DEK transfer;
- target local authority из mounted view;
- telemetry/dashboard как source of truth;
- signature/hash/URI как capability;
- hidden reasoning capture.

## 11. Audit disposition

```text
consistent                    = 25 findings
terminology-only correction   = 10 findings
behavioral conflict           = 0 findings
intentional historical context = 3 findings
owned by future design        = 2 findings
```

Все terminology-only corrections имеют нормативное разрешение в этом отчёте и в уже merged approved designs.

Unresolved behavioral conflicts отсутствуют.

## 12. Review gate

Слияние этого отчёта утверждает только cross-plan ownership и terminology normalization.

Оно не разрешает:

- Rust implementation;
- migrations;
- generated schemas;
- webhook endpoints;
- key generation/rotation/erasure;
- Run export/import execution;
- cross-workspace grants/mounts;
- capture activation;
- event upcasters;
- новую implementation branch.

После письменного утверждения следующий этап — отдельный implementation plan для каждого approved concern, по одному concern на PR.