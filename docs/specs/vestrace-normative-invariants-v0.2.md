# Vestrace Normative Invariants Catalog v0.2

**Статус:** normative requirements baseline  
**Дата:** 2026-08-10  
**Зависимости:**

- `docs/specs/vestrace-architecture-contract-v0.2.md`
- `docs/specs/vestrace-domain-model-v0.2.md`

> Каталог определяет стабильные requirement IDs для требований, которые позднее должны получить conformance tests/evidence. Он не утверждает, что текущая реализация уже проходит эти требования.

## 1. Формат

Каждое требование имеет:

- **ID** — стабильный идентификатор;
- **Level** — MUST / SHOULD / MAY;
- **Class** — тип проверки;
- **Statement** — нормативное требование.

Verification classes:

- `STATIC` — schema/config/manifest inspection;
- `DOMAIN` — deterministic domain rule;
- `STATEFUL` — последовательность состояний/mutations;
- `SECURITY` — authorization/isolation negative/positive tests;
- `FAULT` — controlled failure injection;
- `RECOVERY` — crash/rebuild/revalidation;
- `INTEROP` — adapter/protocol/federation interoperability;
- `EVIDENCE` — наличие provenance/audit/verification evidence.

---

# 2. Architecture (`ARC-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| ARC-001 | MUST | STATIC | Vestrace должен различать authoritative/canonical state и derived state. |
| ARC-002 | MUST | STATEFUL | Derived state должен иметь rebuild path из более авторитетного слоя. |
| ARC-003 | MUST NOT | STATEFUL | Derived state не может автоматически переписывать более авторитетный state только ради устранения drift. |
| ARC-004 | MUST | STATIC | Repair, recovery, governance и external-effect workflows должны использовать единый execution/policy/audit boundary. |
| ARC-005 | MUST NOT | STATIC | Архитектура не должна вводить второй event store или параллельный orchestration runtime без отдельного ADR. |
| ARC-006 | MUST | EVIDENCE | Исправления history должны сохранять факт исходного состояния/события. |
| ARC-007 | MUST | STATEFUL | Неопределённость должна иметь явное state representation и не сводиться автоматически к success/failure. |
| ARC-008 | MUST | STATIC | Identity/reference/hash/URI не должны сами по себе предоставлять authorization. |
| ARC-009 | SHOULD | STATIC | Новые durable entities должны иметь typed IDs и явного authority owner. |
| ARC-010 | MUST | STATIC | Любая новая projection должна быть обозначена как projection и не становиться source of truth по умолчанию. |

---

# 3. Persistent Cognition / Memory (`MEM-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| MEM-001 | MUST | DOMAIN | `Memory` имеет стабильную identity, а semantic content хранится в immutable revisions. |
| MEM-002 | MUST | DOMAIN | Изменение semantic content создаёт новую `MemoryRevision`; in-place overwrite запрещён. |
| MEM-003 | MUST | DOMAIN | Active revision должна принадлежать той же Memory. |
| MEM-004 | MUST | DOMAIN | Revision numbers должны быть монотонны и неизменяемы внутри Memory. |
| MEM-005 | MUST | DOMAIN | Active Memory должна иметь как минимум один admissible evidence/source reference. |
| MEM-006 | MUST | DOMAIN | Derived Memory должна иметь Derivation. |
| MEM-007 | MUST | STATEFUL | `Superseded`, `Expired`, `Rejected` и `Deleted` не должны молча возвращаться в `Active`. |
| MEM-008 | MUST | DOMAIN | `valid_until`, если задан, не может быть раньше `valid_from`. |
| MEM-009 | MUST | DOMAIN | Confidence/importance должны быть finite и находиться в диапазоне `[0,1]`. |
| MEM-010 | MUST | STATIC | Structured payload должен иметь schema identity/version. |
| MEM-011 | MUST | DOMAIN | Scope relation не может расширить workspace authority boundary. |
| MEM-012 | MUST | EVIDENCE | Derived canonical cognition должна иметь provenance closure до source evidence либо явной human-authored authority. |
| MEM-013 | MUST | DOMAIN | `Claim` и `Memory` должны оставаться различимыми domain concepts. |
| MEM-014 | MUST | DOMAIN | Claim status `Supported` не должен трактоваться как абсолютная внешняя истина. |
| MEM-015 | MUST | STATEFUL | Claim, потерявший всё admissible supporting evidence, должен перейти через revalidation. |
| MEM-016 | MUST | STATEFUL | Open semantic conflict должен оставаться явным до reconciliation. |
| MEM-017 | MUST NOT | STATEFUL | Conflict не должен автоматически разрешаться универсальным `latest timestamp wins`. |
| MEM-018 | MUST | EVIDENCE | Conflict resolution должен сохранять basis/evidence/policy/human decision. |
| MEM-019 | MUST | STATEFUL | Supersession не должен удалять superseded history. |
| MEM-020 | SHOULD | STATIC | Cognitive assets (agents/skills/workflows) должны быть отдельными versioned assets, а не Memory records. |

---

# 4. Temporal & Concurrency (`TMP-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| TMP-001 | MUST | DOMAIN | `recorded_at` и `occurred_at` должны иметь различимую семантику. |
| TMP-002 | MUST | DOMAIN | Unknown `occurred_at` не должен блокировать authoritative recording, если `recorded_at` известен. |
| TMP-003 | MUST | DOMAIN | Knowledge validity (`valid_from/to`) должна быть независима от recording time. |
| TMP-004 | MUST | STATEFUL | Current retrieval не должен выдавать superseded/expired knowledge как current. |
| TMP-005 | MUST | STATEFUL | `as_of` query должна использовать соответствующую historical state semantics, а не current projection. |
| TMP-006 | MUST | DOMAIN | Canonical mutation должна иметь expected version/revision precondition, когда возможна конкуренция. |
| TMP-007 | MUST | STATEFUL | Version mismatch должен давать conflict/stale result без silent overwrite. |
| TMP-008 | MUST NOT | DOMAIN | Wall-clock timestamp не должен использоваться как доказательство глобального causal order без дополнительного ordering contract. |
| TMP-009 | MUST | DOMAIN | Lease/heartbeat не должен сам по себе повышать authority или менять logical revision. |
| TMP-010 | SHOULD | STATEFUL | Canonical append-only streams должны иметь проверяемый monotonic order внутри aggregate/scope. |

---

# 5. Mutation & Reconciliation (`MUT-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| MUT-001 | MUST | EVIDENCE | Значимая cognitive mutation должна сохранять actor, target, expected state, reason/intent и resulting revision/fact. |
| MUT-002 | MUST NOT | STATEFUL | Correction не должна переписывать старую revision. |
| MUT-003 | MUST | DOMAIN | Deterministic reconciliation должен использовать более авторитетный source/state. |
| MUT-004 | MUST | DOMAIN | Semantic ambiguity не должна маскироваться под deterministic repair. |
| MUT-005 | MUST | STATEFUL | Policy-guided reconciliation должна ссылаться на exact policy version. |
| MUT-006 | MUST | STATEFUL | Human-required reconciliation не должна исполняться автоматически. |
| MUT-007 | MUST | STATIC | State Engine не должен вводить generic Task/Action aggregates, дублирующие существующих владельцев execution semantics. |
| MUT-008 | MUST | EVIDENCE | Reconciliation result должен сохранять evidence/basis и relation к conflicting inputs. |

---

# 6. Retrieval / ContextPack (`RET-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| RET-001 | MUST | SECURITY | Security/workspace/classification/share filters должны применяться до disclosure запрещённого content downstream ranking/model stages. |
| RET-002 | MUST | STATEFUL | Current retrieval должен исключать superseded/expired cognition как current truth. |
| RET-003 | MUST | STATEFUL | Unresolved conflicts должны возвращаться явно, а не превращаться в выбранный факт без reconciliation. |
| RET-004 | MUST | DOMAIN | `ContextPack.used_budget` не должен превышать hard token/content budget. |
| RET-005 | MUST | EVIDENCE | Каждый ContextPack item должен сохранять source/revision provenance reference. |
| RET-006 | MUST | EVIDENCE | Каждый ContextPack item должен иметь inclusion explanation достаточную для диагностики. |
| RET-007 | MUST | SECURITY | ContextPack должен соблюдать classification/model-destination policy. |
| RET-008 | MUST NOT | DOMAIN | Включение item в ContextPack не должно повышать его semantic authority. |
| RET-009 | MUST | STATEFUL | Failure derived retrieval channel должен приводить к explicit degraded mode, если безопасный fallback существует. |
| RET-010 | MUST | STATEFUL | Если безопасного fallback нет, retrieval должен fail closed. |
| RET-011 | SHOULD | DOMAIN | Несопоставимые channel scores следует объединять rank-based fusion, а не прямым суммированием. |
| RET-012 | MUST | STATEFUL | Cache/context generation должен инвалидироваться при релевантном canonical/policy/share/classification change. |
| RET-013 | MUST | DOMAIN | Representation levels `Full/Summary/Atomic/Reference` должны сохранять ссылку на original source/revision. |
| RET-014 | MUST | SECURITY | Cross-workspace mounted content должен проходить source и target policy checks при каждом governed access. |
| RET-015 | MUST | EVIDENCE | Retrieval run должен быть journalable с policy/version/degradation metadata. |

---

# 7. Feedback & Learning (`LRN-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| LRN-001 | MUST | EVIDENCE | Feedback signal должен ссылаться на exact execution/model/tool/policy revisions. |
| LRN-002 | MUST | DOMAIN | Raw evaluation facts должны храниться отдельно от learned projections/recommendations. |
| LRN-003 | MUST NOT | SECURITY | Learning pipeline не должен автоматически повышать capabilities/permissions. |
| LRN-004 | MUST NOT | SECURITY | Learning pipeline не должен обходить governance policy при изменении agents/skills/workflows. |
| LRN-005 | MUST | STATEFUL | Изменение cognitive assets по результатам learning должно быть versioned/provenanced. |
| LRN-006 | SHOULD | DOMAIN | Deterministic/human signals должны иметь более высокий default authority, чем model-judge heuristic signals. |
| LRN-007 | MUST | EVIDENCE | Learned performance conclusion должен сохранять ссылки на underlying measurements. |
| LRN-008 | MUST | STATEFUL | Удаление learned projection не должно уничтожать raw execution/evaluation history. |

---

# 8. Capability Governance (`CAP-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| CAP-001 | MUST | SECURITY | Runtime authorization определяется effective capability/policy, а не role name. |
| CAP-002 | MUST | SECURITY | Default policy — deny. |
| CAP-003 | MUST | STATIC | Capability должна поддерживать restriction по operation/tool, resource/scope, validity, budget, risk и conditions. |
| CAP-004 | MUST | SECURITY | Expired capability должна быть rejected. |
| CAP-005 | MUST | SECURITY | Child/subagent effective authority не может быть шире parent effective authority. |
| CAP-006 | MUST | SECURITY | Delegation требует явного delegation permission/contract. |
| CAP-007 | MUST | SECURITY | Delegation depth должна быть bounded. |
| CAP-008 | MUST | DOMAIN | Risk categories как минимум `low/medium/high/critical`. |
| CAP-009 | MUST | DOMAIN | Context может повысить effective risk. |
| CAP-010 | MUST NOT | SECURITY | Approval не должен преодолевать explicit deny, hard budget или capability ceiling. |
| CAP-011 | MUST | EVIDENCE | Policy decision должен сохранять exact policy version и relevant input state. |
| CAP-012 | MUST | SECURITY | Material change approved operation/intent должен инвалидировать approval. |
| CAP-013 | SHOULD | EVIDENCE | Budget reservations/accounting должны быть auditable. |
| CAP-014 | MUST | SECURITY | Agent-facing API не должен предоставлять self-escalation capability. |

---

# 9. Identity / Workspace / Federation (`IDW-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| IDW-001 | MUST | SECURITY | Workspace является authority/isolation boundary. |
| IDW-002 | MUST | SECURITY | Workspace-local `global` не должен означать cross-workspace global. |
| IDW-003 | MUST | SECURITY | Actor identity и authority должны проверяться независимо. |
| IDW-004 | MUST | SECURITY | Cross-workspace Memory access требует explicit source grant и target acceptance. |
| IDW-005 | MUST NOT | SECURITY | Wildcard target grants запрещены. |
| IDW-006 | MUST NOT | SECURITY | Mounted memory не может быть автоматически re-shared третьему workspace. |
| IDW-007 | MUST | SECURITY | Source и target policy должны проверяться независимо. |
| IDW-008 | MUST | STATEFUL | Revoked/expired/stale mount не должен выдавать content. |
| IDW-009 | MUST | DOMAIN | `SharedMemoryRef` должен быть namespaced source workspace + exact memory/revision/grant context. |
| IDW-010 | MUST NOT | DOMAIN | SharedMemoryRef не должен подставляться как local MemoryId. |
| IDW-011 | MUST | EVIDENCE | Local derivation/import from mounted content должен сохранять source provenance. |
| IDW-012 | MUST | SECURITY | Federation trust/identity recognition не должен сам по себе разрешать data disclosure. |
| IDW-013 | MUST | STATEFUL | Revocation не должна переписывать исторический факт уже совершённого disclosure. |
| IDW-014 | MUST | SECURITY | Cross-workspace access не должен требовать снятия RLS/normal workspace isolation. |

---

# 10. Health / Integrity / Repair (`HLT-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| HLT-001 | MUST | EVIDENCE | HealthFinding должен содержать invariant, scope, evidence, severity и repairability. |
| HLT-002 | MUST | DOMAIN | Aggregate health является projection, а не canonical truth. |
| HLT-003 | MUST | DOMAIN | `UNKNOWN` health не должен трактоваться как `HEALTHY`. |
| HLT-004 | MUST | DOMAIN | Severity и repair risk должны быть отдельными dimensions. |
| HLT-005 | MUST | STATIC | Invariant definition должен иметь stable ID и version. |
| HLT-006 | MUST | STATEFUL | Deterministic auto-repair разрешён только из более авторитетного source/state. |
| HLT-007 | MUST NOT | STATEFUL | Checker не должен напрямую выполнять hidden mutation. |
| HLT-008 | MUST | STATEFUL | Repair должен проходить immutable RepairPlan до execution. |
| HLT-009 | MUST | STATEFUL | RepairPlan с stale preconditions должен получить `STALE_PLAN` и не исполняться. |
| HLT-010 | MUST | SECURITY | Repair execution должен проходить capability/policy gates согласно risk/repairability. |
| HLT-011 | MUST | STATEFUL | `RepairExecution=SUCCEEDED` не должен автоматически закрывать finding. |
| HLT-012 | MUST | EVIDENCE | Finding `RESOLVED` требует successful verification evidence. |
| HLT-013 | MUST | STATEFUL | Verification failure должен оставить finding open/reopened. |
| HLT-014 | MUST | STATEFUL | Occurrences одного logical finding должны сохраняться отдельно от finding identity. |
| HLT-015 | MUST | STATEFUL | Auto-repair должен иметь retry/attempt budget и cooldown. |
| HLT-016 | MUST | STATEFUL | Flapping должен прекращать бесконечный auto-repair loop. |
| HLT-017 | MUST | DOMAIN | Health propagation должна следовать declared dependencies, а не глобальному worst-state rule. |
| HLT-018 | MUST | EVIDENCE | Suppression/AcceptedRisk должны иметь actor, reason, expiry/policy и audit. |
| HLT-019 | MUST NOT | DOMAIN | Suppression не должен удалять finding или превращать raw integrity в healthy. |
| HLT-020 | SHOULD | STATEFUL | Repair steps должны быть idempotent/restartable там, где это технически возможно. |

---

# 11. External Effects (`EXT-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| EXT-001 | MUST | STATEFUL | Immutable ExternalEffectIntent должен быть сохранён до external dispatch. |
| EXT-002 | MUST | SECURITY | Effect должен пройти capability/policy/budget authorization до dispatch. |
| EXT-003 | MUST | DOMAIN | Adapter должен объявлять delivery semantics: at-most-once/at-least-once/effectively-once/unknown. |
| EXT-004 | MUST NOT | STATIC | Adapter/runtime не должен заявлять universal exactly-once без доказуемого provider contract. |
| EXT-005 | MUST | DOMAIN | Idempotency/retry safety должна быть adapter contract, а не свободное решение агента. |
| EXT-006 | MUST | STATEFUL | Transport timeout не должен автоматически означать, что effect не произошёл. |
| EXT-007 | MUST | STATEFUL | `UNKNOWN` outcome должен быть first-class state. |
| EXT-008 | MUST NOT | STATEFUL | UNKNOWN external effect не должен автоматически переводиться в FAILED. |
| EXT-009 | MUST NOT | STATEFUL | UNKNOWN effect не должен автоматически retry, если adapter не доказал retry-safe semantics. |
| EXT-010 | MUST | STATEFUL | Reconciliation должен использовать strongest available read-back/evidence mechanism. |
| EXT-011 | MUST | DOMAIN | `REVERSIBLE`, `COMPENSATABLE`, `IRREVERSIBLE`, `UNKNOWN_REVERSIBILITY` должны быть различимы. |
| EXT-012 | MUST NOT | DOMAIN | Compensation не должна называться rollback. |
| EXT-013 | MUST | EVIDENCE | Compensation должна ссылаться на compensated effect. |
| EXT-014 | MUST | STATEFUL | Critical external preconditions должны проверяться непосредственно перед dispatch. |
| EXT-015 | MUST | SECURITY | Material mutation approved intent требует новой authorization. |
| EXT-016 | MUST | EVIDENCE | Dispatched effect должен иметь receipt/evidence record. |
| EXT-017 | MUST NOT | EVIDENCE | Receipt/ACK не должен автоматически считаться desired business outcome. |
| EXT-018 | MUST | SECURITY | Secret material не должен сохраняться в effect traces/receipts в plaintext. |

---

# 12. Incident / Recovery / Revalidation (`REC-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| REC-001 | MUST | DOMAIN | HealthFinding и Incident должны быть разными concepts. |
| REC-002 | MUST | STATEFUL | Incident lifecycle должен включать containment до trust restoration. |
| REC-003 | MUST | STATEFUL | Process restart не должен автоматически возвращать scope в TRUSTED. |
| REC-004 | MUST | RECOVERY | Startup recovery должен классифицировать unfinished work, а не resume-all. |
| REC-005 | MUST | RECOVERY | Ambiguous external dispatch после crash должен переходить в UNKNOWN/reconciliation path. |
| REC-006 | MUST NOT | RECOVERY | Ambiguous irreversible effect не должен автоматически повторяться после restart. |
| REC-007 | MUST | RECOVERY | Recovery point должен ссылаться на доказанно согласованное state/scope position. |
| REC-008 | MUST | RECOVERY | Snapshot должен проходить integrity/schema/continuity validation до использования как recovery source. |
| REC-009 | MUST | RECOVERY | Derived state после restore должен rebuild/revalidate из canonical state. |
| REC-010 | MUST | DOMAIN | `AVAILABLE`, `HEALTHY`, `TRUSTED`, `RECOVERED`, `REVALIDATED` должны оставаться различимыми. |
| REC-011 | MUST | RECOVERY | Trust не может перейти `UNTRUSTED → TRUSTED` без successful required RevalidationRun. |
| REC-012 | MUST | EVIDENCE | RevalidationRun должен сохранять scope, level, checks, evidence и result. |
| REC-013 | MUST | DOMAIN | `INCONCLUSIVE` revalidation должен быть допустимым результатом и не считаться PASSED. |
| REC-014 | MUST NOT | RECOVERY | Divergent histories не должны автоматически merge через `latest wins`. |
| REC-015 | MUST | RECOVERY | Recovery automation должен иметь bounded retry/budget и обнаруживать recovery loops. |
| REC-016 | SHOULD | EVIDENCE | Перед destructive recovery следует сохранять forensic evidence. |
| REC-017 | MUST | STATEFUL | Capability restoration после trust incident должна следовать policy и MAY быть progressive. |
| REC-018 | MUST | EVIDENCE | Recovery не должен переписывать history так, будто failure/incident не происходил. |

---

# 13. Crypto / Data Governance (`GOV-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| GOV-001 | MUST | STATIC | Crypto controls и DataPolicy controls должны быть независимыми gates. |
| GOV-002 | MUST | DOMAIN | Base sensitivity ordering: PUBLIC < INTERNAL < CONFIDENTIAL < RESTRICTED. |
| GOV-003 | MUST | STATEFUL | Derived data не должен автоматически понижать classification относительно источников. |
| GOV-004 | MUST | EVIDENCE | Declassification должен быть отдельным governed decision с provenance. |
| GOV-005 | MUST NOT | SECURITY | Secret plaintext не должен храниться как ordinary Memory. |
| GOV-006 | MUST | SECURITY | Durable secret usage должен использовать opaque SecretRef + controlled resolver/backend. |
| GOV-007 | SHOULD | SECURITY | Large/CAS data at rest следует защищать envelope encryption при включённом crypto profile. |
| GOV-008 | MUST | STATIC | Crypto metadata должна поддерживать algorithm/version agility. |
| GOV-009 | MUST | DOMAIN | Hash integrity и signature/authorship должны быть различимыми guarantees. |
| GOV-010 | SHOULD | EVIDENCE | Critical audit history должна поддерживать tamper-evident chaining/checkpoints в TRUSTED profile. |
| GOV-011 | MUST | SECURITY | Capability grant не должен обходить DataPolicy restriction. |
| GOV-012 | MUST | SECURITY | DataPolicy allow не должен обходить отсутствие required capability. |
| GOV-013 | MUST | STATEFUL | Retention lifecycle должен различать ACTIVE/EXPIRED/PENDING_DISPOSAL/DISPOSED. |
| GOV-014 | MUST | EVIDENCE | DataHold должен иметь scope, authority и reason. |
| GOV-015 | MUST | DOMAIN | Logical delete, physical delete и crypto erasure должны быть различимы. |
| GOV-016 | MUST NOT | EVIDENCE | Delete result не должен заявлять уничтожение backup copies, если оно не доказано. |
| GOV-017 | MUST | STATEFUL | Evidence deletion должен инициировать revalidation зависимых claims/derived state. |
| GOV-018 | MUST | SECURITY | Export должен быть governed operation с exact scope/recipient/purpose/classification. |
| GOV-019 | MUST | EVIDENCE | Export bundle должен иметь manifest/provenance/integrity metadata. |
| GOV-020 | MUST | SECURITY | Federation identity/trust не должен сам по себе разрешать classified data transfer. |
| GOV-021 | MUST | SECURITY | Model invocation должен проверять destination/locality/classification/redaction policy. |
| GOV-022 | MUST | SECURITY | RESTRICTED data не должен отправляться provider, запрещённому DataPolicy. |
| GOV-023 | MUST | RECOVERY | Signature/hash/audit-chain/key-compromise anomaly должна влиять на trust и запускать соответствующий incident/revalidation path. |
| GOV-024 | MUST | EVIDENCE | Governance decision должен ссылаться на exact policy version. |
| GOV-025 | MUST NOT | STATEFUL | Новая policy version не должна ретроактивно переписывать историю предыдущего решения. |
| GOV-026 | MUST | EVIDENCE | Deletion считается завершённым только после DeletionVerification согласно заявленной deletion semantics. |

---

# 14. Qualification / Conformance (`QUAL-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| QUAL-001 | MUST | STATIC | Tests, conformance и qualification должны быть отдельными concepts. |
| QUAL-002 | MUST | STATIC | Каждый MUST requirement в заявляемом profile должен иметь verification path. |
| QUAL-003 | MUST | EVIDENCE | Qualification result должен сохранять build/source/config/environment identity. |
| QUAL-004 | MUST | STATIC | Profiles должны иметь dependency closure. |
| QUAL-005 | MUST | STATIC | Installation capability manifest должен явно указывать limitations для заявляемого profile. |
| QUAL-006 | MUST | STATEFUL | Skipped обязательный MUST requirement должен приводить к profile qualification failure. |
| QUAL-007 | MUST | SECURITY | Security/governance hard gate failure не может компенсироваться высоким quality score. |
| QUAL-008 | MUST | FAULT | AUTONOMY/TRUSTED qualification должна включать deterministic crash/fault scenarios. |
| QUAL-009 | MUST | RECOVERY | TRUSTED qualification должна включать recovery + revalidation scenarios. |
| QUAL-010 | SHOULD | STATEFUL | Cognitive qualification должна проверять properties/evidence, а не exact LLM wording. |
| QUAL-011 | MUST | INTEROP | Adapter/backend qualification должна проверять заявленные idempotency/recovery/security semantics. |
| QUAL-012 | MUST | INTEROP | Unsupported compatibility должна быть explicit, а не silent best-effort. |
| QUAL-013 | MUST | EVIDENCE | QualificationBundle должен содержать suite version и per-requirement evidence/results. |
| QUAL-014 | MUST | STATEFUL | Material environment/provider/backend/policy/crypto/schema change должен уметь инвалидировать/stale qualification baseline. |
| QUAL-015 | MUST | STATEFUL | Qualification не является бессрочным сертификатом продукта. |
| QUAL-016 | MUST | SECURITY | Destructive qualification scenarios не должны выполняться против production workspace без специального isolated qualification contract. |
| QUAL-017 | MUST | INTEROP | Federation participant self-asserted `TRUSTED` status не должен приниматься как достаточное доказательство trust. |
| QUAL-018 | MUST | EVIDENCE | v1.0 Trust claim требует успешного TRUSTED profile и опубликованных known limitations. |

---

# 15. Release profile mapping

## v0.2 — Correct

Minimum target requirement families:

- `ARC-*` core laws;
- `MEM-*`;
- `TMP-*`;
- `MUT-*`;
- `RET-*`;
- foundational `CAP-*` security requirements required to protect Memory.

## v0.3 — Learn

Добавляет:

- `LRN-*`;
- cognition benchmarks/properties;
- feedback provenance.

## v0.4 — Govern

Добавляет полный:

- `CAP-*`;
- `IDW-*`;
- governance-facing negative tests.

## v0.5 — Understand

Добавляет:

- `HLT-*`;
- repair verification;
- recurrence/flapping scenarios.

## v0.6 — Connect

Добавляет:

- `EXT-*`;
- adapter/interoperability qualification.

## v1.0 — Trust

Требует применимые:

- `REC-*`;
- `GOV-*`;
- `QUAL-*`;
- полный dependency closure предыдущих profiles.

---

# 16. Change control

1. Requirement ID после появления в release/conformance artifacts SHOULD NOT менять смысл.
2. Уточнение формулировки допустимо без нового ID только если оно не меняет observable obligation.
3. Ослабление или изменение observable semantics требует новой version/replacement requirement и ADR/contract update.
4. Удалённый requirement сохраняется в historical mapping как superseded/retired.
5. Новая specialized specification MUST ссылаться на соответствующие requirement IDs вместо создания дублирующих неидентифицированных MUST statements, где это возможно.
