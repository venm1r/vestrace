# Vestrace State Engine — cross-plan normalization report

**Статус:** предложено для письменного утверждения  
**Дата аудита:** 2026-08-01  
**Репозиторий:** `venm1r/vestrace`  
**Характер изменения:** documentation-only; без Rust-кода, миграций, generated schemas, CI и runtime configuration

> Отчёт фиксирует нормативное чтение уже утверждённых документов. Он не создаёт новый Horizon, runtime, event store, aggregate, public API или implementation scope.

## 1. Итог

```text
unresolved behavioral conflicts  = 0
new runtime authorities          = 0
new Horizons                     = 0
existing source documents edited = 0
normalization report added       = 1
```

Актуальные документы образуют совместимую архитектуру. Найденные неоднозначности относятся к терминологии и предварительному размещению будущего implementation-кода, а не к несовместимому runtime-поведению.

## 2. Утверждённые inputs

| Concern | Нормативный документ | Merge commit |
|---|---|---|
| State Engine reconciliation | `docs/superpowers/specs/2026-07-31-vestrace-state-engine-reconciliation-design.md` | `26d2cd895e110722b20883c5e29315676b86480c` |
| Follow-up program | `docs/superpowers/plans/2026-07-31-vestrace-state-engine-followup-documentation.md` | `692012f10135b65e488f924ef3ad61ba566db194` |
| State Engine boundary | `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md` | `8ca111b6273b3c1f9a613b0730df94329ec11db4` |
| Capture profiles | `docs/superpowers/specs/2026-07-31-vestrace-state-capture-profiles-adr.md` | `24f02ed362b2af0361fcc9d499d275e21257c0e2` |
| Event compatibility | `docs/superpowers/specs/2026-07-31-vestrace-event-schema-compatibility-adr.md` | `3f7a196dfc879f628628b31d9eddb9b18770de10` |
| Signed Run export | `docs/superpowers/specs/2026-08-01-vestrace-signed-run-export-design.md` | `c27e76fda273c6aa82e622f0b47f32e4de69d78a` |
| Webhook extension | `docs/superpowers/specs/2026-08-01-vestrace-webhook-extension-design.md` | `8dfb8c11e8ff2ec9fba5ef95aa158d49d1b27f50` |
| Workspace encryption | `docs/superpowers/specs/2026-08-01-vestrace-workspace-envelope-encryption-design.md` | `9b3ad5c274907f0be7bcf4e195ab0b2858e2e999` |
| Memory sharing | `docs/superpowers/specs/2026-08-01-vestrace-cross-workspace-memory-sharing-design.md` | `5961d0d65c16bc5d067aeaa815d42f00becceaff` |

Merged concern-specific design outranks older planning shorthand for that concern. Он ограничивает owning Horizon, но не заменяет его.

## 3. Классификация

Используются ровно пять категорий:

```text
consistent
terminology-only correction
behavioral conflict
intentional historical context
owned by future design
```

- `consistent` — ownership и поведение совместимы;
- `terminology-only correction` — поведение совместимо, но требуется единое нормативное чтение;
- `behavioral conflict` — документы требуют несовместимого runtime-поведения;
- `intentional historical context` — формулировка описывает рассмотренный и отвергнутый вариант;
- `owned by future design` — базовый документ намеренно оставил extension неактивным, а отдельный approved design определил его контракт.

## 4. Единственная ownership matrix

| Область | Authoritative owner |
|---|---|
| `AgentRun`, `RunStep`, `RunEvent`, Run version, checkpoints, work items, logical replay | H1 |
| Capabilities, contextual policy, approvals, authorization tickets, budgets и quotas | H2 |
| Model routing, executions и attempts | H3 |
| Tool/Sandbox invocation, attempts и external Tool effects | H4 |
| Plans, delegation, SubRuns и handoffs | H5 |
| Memory identity, revisions, provenance, conflicts, lifecycle и write policy | v0.1 Memory Core |
| Context snapshots, Artifact identity/revisions, bytes, evidence, export и purge | H6 |
| Conversations, interactions, human continuation, triggers, source facts и public events | H7 |
| Connections, secret bytes/references, credential leases и cryptographic operations | H8 |
| Packages, profiles и extensions | H9 |
| A2A wire mapping и interoperability gateway | H9A |
| Telemetry projections, audit, evaluation, verification, safe replay и explanations | H10 |
| HTTP/CLI/MCP/SDK/UI, deployment, release, backup/restore и product adapters | H11 |
| `Vestrace State Engine` | composite internal boundary H1–H11; не отдельный owner |

## 5. Findings

| ID | Тема | Классификация | Нормализованное разрешение |
|---|---|---|---|
| N-01 | Термин `State Engine` | terminology-only correction | Внутреннее имя composite durable-state boundary; не service, database, crate, API или H12. |
| N-02 | Несколько journals | consistent | H1 Run, Memory Core, H7 interaction/public events и H10 audit — разные typed owners, не второй global event store. |
| N-03 | `Run → Task → Step → Action → Attempt` | intentional historical context | Generic mutable hierarchy отвергнута; используются typed records owning subsystems. |
| N-04 | `recovery_required` / `outcome_unknown` | terminology-only correction | Conceptual labels отображаются через H1 durable waiting/degraded state и typed subsystem Unknown/reconciliation records; universal status не добавляется. |
| N-05 | Logical replay / safe replay / import | terminology-only correction | H1 восстанавливает state; H10 side-effect-free оценивает; Run export inertly verifies/inspects/stages. |
| N-06 | Retry external commitment | consistent | Blind retry запрещён при ambiguous completion без exact idempotency или reconciliation. |
| N-07 | Direct SQL | consistent | Только scoped repositories/prepared transactions/read-only projections; model/agent arbitrary SQL запрещён. |
| N-08 | Capture profiles и H10 modes | consistent | Profiles — presets поверх `Disabled < MetadataOnly < StructuredOnly < Redacted < Full`; итоговый mode может только сужаться policy. |
| N-09 | Logging level меняет events | intentional historical context | Canonical domain events стабильны; меняется только optional capture richness. |
| N-10 | Event schema registry | consistent | Source Horizon владеет meaning; repository registry — schemas/upcasters; H11 publishes; extensions/models/webhooks не регистрируют canonical kinds. |
| N-11 | Journal rewriting | consistent | Historical rows неизменяемы; compatibility — pure deterministic read-time upcasters. |
| N-12 | Любой result как Artifact | terminology-only correction | H6 Artifact существует только после materialization, classification, quarantine/inspection и immutable revision finalization. |
| N-13 | Artifact mount и MemoryMount | terminology-only correction | Разные exact contracts; права и lifecycle не наследуются и не объединяются generic mount aggregate. |
| N-14 | Run export и backup/restore | consistent | Export — inert evidence одного Run; H11 backup — fenced disaster recovery deployment identity. |
| N-15 | Signature как authority | consistent | Подпись доказывает integrity под trust anchor, но не capability, approval или semantic truth. |
| N-16 | Webhook ownership H7/H11 | terminology-only correction | H7 владеет source/trigger/public-event semantics; H11 — management/inspection/transports; H2/H8/H10 сохраняют свои authority. |
| N-17 | Inbound webhook activation | owned by future design | H7 `ExternalEvent` остаётся inactive до отдельного implementation plan approved webhook extension. |
| N-18 | Outbound webhook H11 | consistent | At-least-once intent/attempt, immutable source identity, H8 signing, dedup, SSRF controls и OutcomeUnknown совместимы. |
| N-19 | Webhooks enabled by release | terminology-only correction | Наличие code/fixture/surface не активирует feature; deployment и workspace default — `Disabled`. |
| N-20 | H8 secret encryption и payload encryption | consistent | Разные cryptographic domains: H8 secret backend защищает credentials; envelope encryption защищает selected payloads. |
| N-21 | Plaintext fallback | consistent | Key-backend outage fail-closed; optional capture может стать no-payload, но plaintext не сохраняется. |
| N-22 | KEK/DEK/algorithm rotation | terminology-only correction | KEK rotation = rewrap; DEK/algorithm change = новая encrypted generation. |
| N-23 | Erasure и physical purge | consistent | Разные lifecycle operations; unknown key-destroy result остаётся `OutcomeUnknown`. |
| N-24 | Memory scope `global` | consistent | Всегда workspace-global; sharing не расширяет scope существующей Memory. |
| N-25 | Cross-workspace memory | owned by future design | Только exact source grant + immutable revision + exact target mount acceptance; implementation пока не разрешён. |
| N-26 | Federated retrieval и RLS | consistent | Отдельные source-authorized queries под source RLS; raw cross-workspace SQL join запрещён. |
| N-27 | Mounted memory authority | consistent | Foreign/untrusted view; не local Memory, не local confidence, не auto-supersede, не re-share. |
| N-28 | Derivation/import mounted memory | consistent | Только explicit proposal, source authorization, target policy, normal Memory Core lifecycle и full provenance. |
| N-29 | Revoke/purge и active Run | consistent | Future access/context invalidated; historical journal не переписывается; stale context не переисполняется автоматически. |
| N-30 | Telemetry как source of truth | consistent | H10 operational telemetry lossy; не управляет Run, policy, budget, billing или completion. |
| N-31 | Critical и analytical projections | consistent | Critical state — owning transaction; analytical read models могут lag/rebuild. |
| N-32 | Run completion verification | consistent | H1 владеет terminal transition; H10 предоставляет exact one-use verification evidence. |
| N-33 | Roles и capabilities | consistent | Role/template описывает поведение; authority — H2 capability/policy/ticket exact operation. |
| N-34 | Delegation rights | consistent | Только explicit narrowed grants; automatic inheritance и widening запрещены. |
| N-35 | H11 integration authority | consistent | H11 добавляет product operations, но не второй Run/model/tool/memory/Artifact/policy/audit aggregate. |
| N-36 | Schema publication | consistent | H11 публикует external schemas; source meaning и registry ownership остаются у owning contracts. |
| N-37 | Hidden reasoning | consistent | Не является required persistence и запрещён для capture/export/sharing/webhooks; encryption не меняет запрет. |
| N-38 | Secrets в events/exports | consistent | Secret values остаются за H8; records используют opaque refs и content-free facts. |
| N-39 | Cross-workspace Run export import | consistent | Source IDs остаются provenance; local authority/Run/memory/mount автоматически не создаются. |
| N-40 | Migration ownership | consistent | Design phase не резервирует migrations; ranges назначаются отдельным implementation plan после полного range audit. |

## 6. Проверка обязательных invariants

| Invariant | Результат | Evidence reading |
|---|---|---|
| Один Run owner | PASS | H1 `AgentRun`/journal/checkpoint authority |
| Один final authorization owner | PASS | H2 final policy/approval/budget decision |
| Один Artifact/Context byte owner | PASS | H6 lifecycle/blob ports |
| Один credential/key-operation owner | PASS | H8 SecretBackend/Credential Broker |
| Один audit/evaluation owner | PASS | H10 durable audit/evaluation contracts |
| Нет direct agent SQL | PASS | typed application ports + forced RLS repositories |
| Нет H12 State Engine | PASS | approved boundary amendment |
| Нет automatic side-effect replay | PASS | typed Unknown/reconciliation rules |
| Нет implicit cross-workspace sharing | PASS | exact grant/revision/mount contract |
| Нет второго schema authority | PASS | source meaning + repository registry + H11 publication |

## 7. Почему существующие документы не изменены

Подтверждённых несовместимых runtime requirements не обнаружено, поэтому source plans не переписываются.

Две наиболее заметные неоднозначности разрешаются без изменения behavior:

1. H11 `webhook/*` paths считаются предварительным product-surface placement, но не новым owner H7 trigger/public-event semantics.
2. Слова `replay` и `import` всегда читаются через exact owner: H1 logical replay, H10 safe evaluation replay или inert Run export import.

Массовая вставка ссылок в H1–H11 увеличила бы diff без изменения контракта. Настоящий отчёт является единым normalization/precedence index для следующих implementation plans.

## 8. Требования к следующим implementation plans

Каждый plan обязан:

- указать owning Horizon и не создавать H12;
- сослаться на merged design path и commit;
- использовать существующие typed aggregates вместо generic Task/Action/Attempt;
- определить exact files, ports, migrations и ownership boundaries;
- поставить failing tests раньше implementation steps;
- включить restart, idempotency, reconciliation, RLS и capability tests;
- не разрешать implementation самим фактом merge plan;
- остановиться до production-code phase без explicit user instruction.

| Plan | Обязательная интеграция |
|---|---|
| Capture profiles | H10 capture lattice + H2/H6 policy/export gates |
| Event compatibility | source Horizons + H1/H7/H10/H11 boundaries |
| Signed Run export | H1 snapshot + H2 auth + H6 bytes + H8 signing + H10 audit + H11 adapters |
| Webhooks | H7 source/public events + H2 auth + H8 signing + H10 audit + H11 transports |
| Workspace encryption | H6 lifecycle + H8 key operations + H2 admin auth + H10 content-free audit |
| Memory sharing | Memory Core + H2 + H6 + H8 + H10 + H11 |

## 9. Запрещённые reintroductions

Будущие планы и код не должны вводить:

- H12 State Engine;
- второй global event store;
- generic mutable Task/Action/Attempt authority;
- agent/model direct SQL;
- event suppression по capture profile;
- historical journal rewriting;
- implicit side-effect retry;
- Run export/import, автоматически запускающий Run;
- webhook, напрямую меняющий Run/approval/memory;
- plaintext fallback;
- wildcard memory grant;
- mount-of-mount или transitive sharing;
- source KEK/DEK transfer;
- local authority из mounted view;
- telemetry/dashboard как truth;
- signature/hash/URI как capability;
- hidden reasoning capture.

## 10. Audit disposition

```text
consistent                     = 28
terminology-only correction    = 8
behavioral conflict            = 0
intentional historical context = 2
owned by future design         = 2
-----------------------------------
total findings                 = 40
```

Все terminology-only corrections имеют нормативное разрешение в этом отчёте и merged approved designs.

Unresolved behavioral conflicts отсутствуют.

## 11. Review gate

Слияние утверждает только cross-plan ownership и terminology normalization.

Оно не разрешает Rust implementation, migrations, generated schemas, webhook endpoints, key operations, real Run export/import, cross-workspace grants/mounts, capture activation или event upcasters.

Следующий этап после письменного утверждения — отдельный implementation plan для каждого approved concern, по одному concern на PR.