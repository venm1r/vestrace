# Vestrace — cross-workspace memory sharing design

**Статус:** предложено для письменного утверждения  
**Дата:** 2026-08-01  
**Репозиторий:** `venm1r/vestrace`  
**Базовый memory contract:** `docs/superpowers/specs/2026-07-31-vestrace-v0.1-design.md`  
**Memory Core:** `docs/superpowers/plans/2026-07-31-vestrace-memory-core.md`  
**State Engine boundary:** `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`  
**Envelope encryption:** `docs/superpowers/specs/2026-08-01-vestrace-workspace-envelope-encryption-design.md`  
**Signed Run export:** `docs/superpowers/specs/2026-08-01-vestrace-signed-run-export-design.md`  
**Владельцы контрактов:** v0.1 Memory Core, H2, H6, H8, H10 и H11

> Этот документ является documentation-only design. Он не разрешает менять Rust-код, создавать миграции, открывать cross-workspace access, создавать реальные grants/mounts, копировать память, перестраивать индексы или запускать implementation branch.

## 1. Контекст

Базовый Vestrace v0.1 изолирует память по workspace.

Нормативные свойства текущей модели:

- каждая Memory принадлежит одному workspace;
- `global` scope означает только workspace-global;
- cross-workspace foreign keys и relations запрещены;
- PostgreSQL RLS применяет один workspace context на запрос;
- active memory обязана иметь source;
- derived memory обязана иметь Derivation;
- revision и provenance остаются авторитетными;
- retrieval, context pack, FTS, pgvector и caches учитывают workspace boundary;
- наличие UUID, hash или URI не предоставляет право чтения;
- право прочитать Memory и право передать её модели являются разными решениями.

Эта изоляция остаётся безопасным default.

Одновременно Vestrace требуется явный механизм для ограниченных сценариев совместного использования:

- общий набор проверенных процедур для нескольких проектов;
- общая база продуктовых фактов;
- read-only preference/fact library;
- использование lessons и failure patterns между связанными workspace;
- временная передача выбранного знания для одного Run;
- аудитируемый доступ support/evaluation workspace к конкретным memory revisions;
- управляемая миграция или derivation без скрытого копирования.

Прямое снятие RLS, wildcard scopes или глобальная таблица общей памяти разрушили бы базовые инварианты. Поэтому sharing проектируется как отдельная двухсторонняя capability boundary.

## 2. Нормативный принцип

> **Cross-workspace sharing не расширяет scope существующей Memory. Source workspace выдаёт ограниченный `MemoryShareGrant` конкретному target workspace, а target workspace отдельно принимает read-only `MemoryMount`.**

> **Mount является разрешённым представлением source memory, а не локальной копией, локальной authority или новым source of truth.**

> **Любая локальная запись, derivation, export, model use или дальнейшая передача требует отдельного разрешения и создаёт отдельный авторитетный объект с полным provenance.**

## 3. Цели

Design должен обеспечить:

1. source-controlled disclosure;
2. target-controlled acceptance;
3. read-only default;
4. точную привязку к source и target workspace;
5. immutable grant revisions;
6. lifecycle revoke/suspend/expire;
7. отсутствие wildcard sharing;
8. отсутствие transitive sharing;
9. сохранение source provenance и revision identity;
10. federated retrieval без снятия RLS;
11. независимые source и target policy checks;
12. bounded result sets и content budgets;
13. явное разделение discover, read, context use, model use, derive и export;
14. немедленную cache/index invalidation при revoke/purge;
15. отсутствие source key material в target workspace;
16. re-encryption fresh target DEK при разрешённом import;
17. content-free audit с обеих сторон;
18. fail-closed поведение при stale grant, unknown revision или policy mismatch;
19. безопасное поведение для active Runs;
20. совместимость с signed Run export без неявного переноса grant authority.

## 4. Явно не входит

Этот design не является:

- снятием workspace isolation;
- изменением смысла scope `global`;
- cross-workspace SQL join API;
- способом отключить PostgreSQL RLS;
- общей shared database schema для всех workspace;
- автоматическим объединением memory graphs;
- механизмом автоматического копирования памяти;
- автоматическим созданием target embeddings;
- автоматической активацией target memory;
- механизмом выдачи write access к source memory;
- способом редактировать source Memory из target workspace;
- способом передавать secret values;
- способом расширить provider locality или classification policy;
- правом повторно поделиться mounted memory;
- wildcard grant для всех текущих или будущих workspace;
- recursive mount-of-mount;
- способом трактовать foreign confidence как local confidence;
- способом автоматически supersede local knowledge;
- заменой Artifact mounts H6;
- заменой Run export/import;
- механизмом replication PostgreSQL;
- механизмом общего KMS/KEK между workspace;
- способом восстановить purged или cryptographically erased content;
- способом сохранять hidden reasoning.

## 5. Владение контрактами

| Область | Владелец | Роль в sharing |
|---|---|---|
| Memory identity, revision, status, source, derivation, conflict | v0.1 Memory Core | сохраняет source authority и создаёт local memories только через обычный lifecycle |
| Authorization, capabilities, approvals, risk | H2 | разрешает create/accept/read/use/derive/import/export/revoke operations |
| Context packs, large content, classification, purge | H6 | применяет disclosure, context assembly, payload и lifecycle policies |
| Cryptographic operations и key references | H8 | не передаёт source KEK/DEK; обеспечивает target re-encryption при import/cache |
| Audit, usage accounting, diagnostics | H10 | фиксирует source disclosure и target use без raw sensitive content |
| HTTP/CLI/MCP/SDK/UI surfaces | H11 | предоставляет adapters над application contracts |

State Engine не становится владельцем Memory. `MemoryShareGrant` не создаёт второй memory aggregate и не меняет source revision.

## 6. Модель угроз

Design учитывает:

1. target пытается читать память после revoke;
2. target угадывает source UUID;
3. source administrator ошибочно выдаёт wildcard access;
4. mount используется для повторной передачи третьему workspace;
5. target index сохраняет content после revoke/purge;
6. source confidence ошибочно принимается за local trust;
7. mounted memory автоматически supersede local memory;
8. target пытается записать revision в source workspace;
9. query bypass выполняет cross-workspace SQL join;
10. source content попадает remote model без source consent;
11. target сохраняет plaintext или embedding без indexing permission;
12. source KEK/DEK копируется в target;
13. mount ссылается на stale или replaced grant revision;
14. source deletes/erases content, но target продолжает выдавать cache;
15. active Run продолжает использовать revoked context;
16. signed export включает foreign bytes без source export permission;
17. foreign relation создаёт dangling cross-workspace FK;
18. target повторно делится mounted content;
19. grant acceptance подменяется наличием URL/ID;
20. source и target policies расходятся по classification/locality;
21. target создаёт derived memory без source provenance;
22. revocation используется как попытка переписать уже зафиксированную историю;
23. malicious source memory содержит инструкции и повышает свою authority;
24. source workspace компрометирован и выдаёт ложные memories;
25. target query раскрывает sensitive intent source workspace.

Design не гарантирует удаление знания из человеческой памяти или из уже законно раскрытого внешнего результата. Он минимизирует долговременные копии, фиксирует disclosure и управляет системными производными.

## 7. Основные сущности

### 7.1 MemoryShareGrant

`MemoryShareGrant` — mutable lifecycle identity, принадлежащая source workspace.

```text
MemoryShareGrant
├── grant_id
├── source_workspace_id
├── target_workspace_id
├── lifecycle_state
├── active_revision_id?
├── created_by
├── state_revision
├── created_at
└── updated_at
```

Lifecycle state:

```text
Draft
PendingAcceptance
Active
Suspended
Revoked
Expired
Deleted
```

Правила:

- source и target workspace различаются;
- target всегда один exact workspace;
- wildcard target запрещён;
- grant не предоставляет access до target acceptance;
- `Revoked`, `Expired` и `Deleted` не возвращаются в `Active`;
- изменение условий создаёт новую immutable revision;
- lifecycle mutation использует `expected_state_revision`;
- grant ID не является capability.

### 7.2 MemoryShareGrantRevision

```text
MemoryShareGrantRevision
├── grant_revision_id
├── grant_id
├── revision_number
├── source_workspace_id
├── target_workspace_id
├── purpose
├── selection_contract
├── allowed_memory_kinds
├── allowed_statuses
├── classification_ceiling
├── content_mode
├── operation_permissions
├── provider_use_policy
├── indexing_policy
├── derivation_policy
├── export_policy
├── downstream_obligations
├── result_limits
├── valid_from
├── valid_until
├── policy_decision_id
├── approval_refs[]
├── canonical_hash
├── created_by
└── created_at
```

Revision неизменяема.

Изменение любого из следующих условий создаёт новую revision:

- target workspace;
- selection;
- kind/status/classification;
- content mode;
- operation permission;
- provider/model use;
- indexing;
- derivation/import/export;
- limits;
- downstream obligations;
- expiry;
- source policy decision.

### 7.3 MemoryMount

`MemoryMount` принадлежит target workspace и представляет принятие exact grant revision.

```text
MemoryMount
├── mount_id
├── target_workspace_id
├── source_workspace_id
├── grant_id
├── accepted_grant_revision_id
├── display_name
├── lifecycle_state
├── target_policy_revision_id
├── accepted_by
├── state_revision
├── mount_generation
├── created_at
└── updated_at
```

Lifecycle state:

```text
Pending
Active
Suspended
Stale
Revoked
Expired
Deleted
```

`Stale` означает, что source grant получил новую revision, был suspended либо target policy больше не совпадает. Stale mount не выдаёт content до явного re-accept.

### 7.4 SharedMemoryRef

Cross-workspace reference не является обычным `MemoryId`.

```text
SharedMemoryRef
├── source_workspace_id
├── memory_id
├── memory_revision_id
├── grant_revision_id
└── source_generation
```

Reference всегда namespaced. Target не создаёт foreign key из local `memories` к source table.

`SharedMemoryRef`:

- не предоставляет access;
- не гарантирует, что content ещё доступен;
- не может быть подставлен в local `MemoryId`;
- сохраняется в provenance/audit/context manifests;
- проверяется при каждом content access;
- может оставаться tombstone после source purge.

### 7.5 MountedMemoryView

```text
MountedMemoryView
├── shared_ref
├── source_kind
├── source_status
├── source_confidence
├── source_importance
├── source_validity
├── source_provenance_summary
├── source_classification
├── disclosed_content
├── disclosure_mode
├── source_policy_revision_id
├── target_policy_revision_id
├── retrieved_at
└── use_constraints
```

Это transient/read model. Он не является local Memory и не получает local lifecycle status.

### 7.6 MemoryImportProposal

Локальное копирование или создание derivative не происходит через mount read.

```text
MemoryImportProposal
├── proposal_id
├── target_workspace_id
├── source_ref
├── source_grant_revision_id
├── requested_operation
├── proposed_kind
├── proposed_content
├── proposed_classification
├── provenance_plan
├── downstream_obligations
├── source_authorization_ref
├── target_policy_decision_id
├── status
├── created_by
└── created_at
```

Requested operation:

```text
DeriveLocalMemory
ImportExactContent
CreateSummary
CreateProcedureVariant
CreateConflictRecord
```

Status:

```text
Candidate
Approved
Rejected
Expired
Committed
Cancelled
```

Commit выполняется через обычный Memory Core write lifecycle и не редактирует source memory.

## 8. Двухстороннее создание sharing relationship

Нормативный flow:

```text
source principal proposes grant revision
→ H2 source authorization
→ H6 classification/disclosure evaluation
→ optional source approval
→ grant = PendingAcceptance
→ target inspects bounded metadata
→ H2 target authorization
→ target policy evaluation
→ optional target approval
→ MemoryMount accepts exact revision
→ grant/mount become Active atomically or through reconciled handshake
```

Source не может активировать mount единолично. Target не может расширить grant при acceptance.

### 8.1 Source authorization

Минимальные capabilities:

```text
memory.share.create
memory.share.revise
memory.share.suspend
memory.share.revoke
```

Capability scoped к source workspace и exact target workspace.

### 8.2 Target authorization

Минимальные capabilities:

```text
memory.mount.inspect
memory.mount.accept
memory.mount.suspend
memory.mount.remove
```

Target acceptance фиксирует:

- exact grant revision hash;
- target policy revision;
- target principal;
- accepted purpose;
- accepted downstream obligations;
- local display/usage constraints.

### 8.3 Activation race

Если source меняет/suspends/revokes revision между inspection и acceptance, acceptance завершается conflict. Target не активирует stale revision.

### 8.4 Expiry

`valid_until` рекомендуется для всех grants и обязателен для:

- Restricted content;
- support/evaluation access;
- provider/model use;
- export permission;
- import exact content;
- temporary Run-scoped sharing.

Expiry использует trusted server time. Expired grant не выдаёт content даже при stale cache.

## 9. Selection contract

Grant не использует arbitrary SQL, code или unbounded query.

Допустимые selection modes:

```text
ExplicitRevisionSet
ExplicitMemorySetCurrentRevision
NamedSourceCollection
BoundedTypedFilter
RunScopedReferenceSet
```

### 9.1 ExplicitRevisionSet

Наиболее строгий режим. Grant содержит exact list `MemoryRevisionId` и source generation/hash.

Используется для:

- support case;
- evaluation fixture;
- regulatory disclosure;
- deterministic Run context;
- migration staging.

### 9.2 ExplicitMemorySetCurrentRevision

Grant содержит exact `MemoryId`, но разрешает читать current revision при каждом access.

Source revision updates автоматически видимы только если:

- status/kind/classification остаются допустимыми;
- source policy разрешает disclosure новой revision;
- mount не stale;
- target policy принимает новую classification;
- result limits соблюдены.

### 9.3 NamedSourceCollection

Source-owned immutable collection revision содержит explicit members или bounded deterministic filter. Target не может менять collection.

### 9.4 BoundedTypedFilter

Допустимые predicates:

- `MemoryKind` set;
- source status set;
- explicit tags from controlled vocabulary;
- validity window;
- created/updated time range;
- exact source scope kinds;
- minimum/maximum confidence/importance;
- explicit provenance source classes;
- explicit classification ceiling.

Запрещено:

- raw SQL;
- arbitrary JSONPath;
- executable expression;
- model-generated predicate без validation;
- unbounded regex;
- target-controlled source workspace widening;
- filter, который включает future unknown kinds автоматически.

### 9.5 RunScopedReferenceSet

Grant разрешает exact набор revisions для одного Run или Run family. Завершение/expiry Run не обязательно удаляет audit reference, но прекращает future content access.

## 10. Operation permissions

Grant разделяет операции.

```text
DiscoverMetadata
ReadContent
UseInDeterministicContext
UseInModelContext
CreateLocalDerivationProposal
ImportExactContentProposal
IncludeInRunExport
InspectProvenance
InspectRelations
```

Правила:

- `DiscoverMetadata` не подразумевает `ReadContent`;
- `ReadContent` не подразумевает model use;
- deterministic context use и model context use разделены;
- derivation и exact import разделены;
- export является отдельным разрешением;
- relation inspection не позволяет переходить к неразрешённым nodes;
- operation permission всегда пересекается с target H2/H6 policy;
- target не может расширить source grant;
- source grant не может ослабить target restrictions.

Эффективное разрешение:

```text
effective = source_grant
          ∩ source_current_policy
          ∩ target_mount_acceptance
          ∩ target_current_policy
          ∩ principal_capabilities
          ∩ Run/context/provider constraints
```

Любое несовпадение приводит к deny или reduced content mode.

## 11. Content modes

```text
MetadataOnly
StructuredOnly
Redacted
FullAuthorized
```

`FullAuthorized` означает полный content только в пределах classification, purpose, target policy и exact operation.

Source может понизить content mode в момент чтения из-за:

- reclassification;
- redaction policy;
- secret detection;
- legal hold;
- source lifecycle;
- provider locality;
- Run purpose;
- target trust;
- expired approval.

Target не может потребовать более высокий mode, чем grant.

### 11.1 Secret material

Memory с classification `Secret` может содержать только safe reference metadata согласно базовому design. Usable secret values никогда не передаются через mount.

Credential-like content, обнаруженное в обычной Memory:

- блокируется;
- quarantine/reclassification запускается в source workspace;
- target получает bounded denial/tombstone;
- raw value не входит в audit/error.

## 12. Read-only default

Активный mount разрешает только чтение представления.

Target не может через mount:

- создать source revision;
- изменить source status;
- supersede source memory;
- добавить source relation;
- изменить source confidence;
- удалить source memory;
- изменить source scope;
- активировать source candidate;
- разрешить source conflict;
- менять source provenance.

Любая target-side запись создаёт local object через отдельную команду и source provenance reference.

## 13. Federated retrieval

Cross-workspace retrieval не выполняется одним SQL query с отключённым RLS.

Нормативный flow:

```text
target retrieval request
→ target local policy and mount selection
→ local workspace retrieval
→ per-mount source query command
→ source workspace context + source RLS
→ source grant/current-policy check
→ bounded source candidate response
→ target policy/classification check
→ target rerank with origin labels
→ bounded combined result
→ optional context assembly
```

Каждый source query выполняется в source workspace authority context. Target application получает только разрешённый DTO.

### 13.1 Запрет raw cross-workspace SQL

Запрещено:

- setting RLS workspace to target и joining source rows;
- privileged query без grant evaluation;
- exposing database views с несколькими workspace;
- выдавать analytic agent произвольный cross-workspace view;
- подменять grant проверкой UUID existence;
- materialize foreign rows в target `memories` silently.

### 13.2 Query privacy

Target query может раскрывать намерение source workspace. Поэтому grant revision определяет query disclosure mode:

```text
ExactQueryAllowed
RedactedQueryTerms
StructuredFilterOnly
PrecomputedCollectionOnly
```

Sensitive target query не передаётся source без target policy. Source audit может хранить hash/structured categories вместо raw query.

### 13.3 Result limits

Grant задаёт:

- max results per request;
- max total content bytes;
- max structured payload bytes;
- max provenance nodes;
- max relation expansion depth;
- max requests per time window;
- max context tokens;
- maximum query duration;
- optional per-Run budget.

Source limits и target limits пересекаются по более строгому значению.

## 14. Ranking и trust

Mounted memory не получает local trust автоматически.

Combined retrieval сохраняет:

```text
origin = Local | Mounted
source_workspace_id?
source_confidence
source_importance
source_trust_class
mount_trust_ceiling
source_policy_revision
local_interpretation_score
```

### 14.1 Confidence

Source confidence остаётся фактом source workspace. Target reranker может снизить её влияние, но не переписывает source value.

### 14.2 Precedence

Mounted memory:

- не supersede local memory автоматически;
- не разрешает local conflict автоматически;
- не заменяет local constraint/permission;
- не становится higher authority из-за source label;
- не может повысить Run permissions;
- рассматривается как untrusted content для instruction hierarchy.

### 14.3 Conflict detection

Target может создать local `MemoryConflict` или conflict proposal между local revision и `SharedMemoryRef`.

Conflict record хранит namespaced foreign reference, но не foreign key.

Решение конфликта может:

- оставить local memory;
- создать local derived revision;
- принять foreign fact через explicit import proposal;
- пометить foreign result как incompatible;
- запросить human review.

Source memory не меняется.

## 15. Context Pack integration

Mounted content входит в Context Pack только если разрешены:

1. `ReadContent`;
2. соответствующий `UseInDeterministicContext` или `UseInModelContext`;
3. source current policy;
4. target current policy;
5. exact provider/model locality and classification;
6. Run purpose and budget;
7. mount active/non-stale;
8. content mode.

Context manifest фиксирует:

```text
MountedContextEntry
├── shared_ref
├── mount_id
├── grant_revision_id
├── source_disclosure_decision_id
├── target_use_decision_id
├── disclosure_mode
├── content_hash_or_tombstone
├── included_token_count
├── provider_use_allowed
└── assembled_at
```

### 15.1 Active Run и revoke

Если grant/mount revokes после Context Pack assembly:

- новые model/tool calls не используют stale mounted entry;
- context cache invalidируется;
- Run получает durable context dependency invalidation;
- owning Run policy выбирает pause, rebuild или fail-closed;
- уже зафиксированный historical invocation не переписывается;
- audit сохраняет, что content был разрешён на момент assembly;
- future retry не использует старый payload автоматически.

### 15.2 Deterministic replay

Replay может ссылаться на historical content hash/manifest, но не получает bytes после revoke/purge без отдельного lawful retention permission.

Если bytes недоступны:

```text
Replayability = NotReplayable | Inconclusive
```

Vestrace не выдумывает отсутствующий content.

## 16. Indexing policy

Default:

```text
TargetSideIndexing = Disabled
```

Live mount query использует source FTS/pgvector/structured retrieval.

Причины:

- embedding является производным раскрытием content;
- target index создаёт долговременную копию;
- revoke/purge становится сложнее;
- cross-workspace equality может утечь;
- source embedding space может отличаться.

### 16.1 Разрешённый target-side index

Target-side indexing допускается только при explicit grant permission и target acceptance.

Разрешённые modes:

```text
MetadataIndexOnly
EncryptedEmbeddingIndex
EncryptedRedactedTextIndex
PinnedRevisionIndex
```

Требования:

- fresh target DEK;
- no source KEK/DEK sharing;
- index rows помечены mount/grant/source revision;
- target mount generation входит в cache/index key;
- revoke/purge ставит index in quarantine immediately;
- async physical cleanup имеет durable job и audit;
- until cleanup complete results are not queryable;
- content hash не используется для cross-workspace dedup;
- source content не смешивается с local authoritative index без origin field.

### 16.2 Embedding generation

Target embedding generation считается `UseInModelContext` или отдельным `GenerateDerivedIndex` permission в зависимости от provider.

Remote embedding provider требует source и target policy approval.

## 17. Cache и generation invalidation

Cache key включает:

```text
source_workspace_id
grant_revision_id
mount_id
mount_generation
source_memory_generation
target_memory_generation
query_hash_or_structured_filter
content_mode
source_policy_revision
target_policy_revision
provider/locality profile
```

Invalidation triggers:

- source Memory revision/status/classification change;
- source purge/erasure;
- grant revision/lifecycle change;
- target mount lifecycle change;
- source/target policy change;
- indexing permission change;
- provider policy change;
- downstream obligation change;
- expiry;
- source collection revision change.

Stale cache никогда не выдаётся как fallback при недоступности source. Возможен только explicit `StaleMetadataOnly` diagnostic без content и без context use.

## 18. Local derivation и import

### 18.1 DeriveLocalMemory

Target создаёт новую local Memory с:

- новым `MemoryId`;
- local classification;
- local confidence/importance assessment;
- source `SharedMemoryRef`;
- `Derivation` с method и exact inputs;
- source grant revision;
- source/target policy decisions;
- target write policy lifecycle;
- candidate status по умолчанию, если policy не разрешает activation.

Foreign confidence не копируется как local confidence без assessment.

### 18.2 ImportExactContent

Exact import является high-risk отдельной операцией.

Требуется:

```text
source permission = ImportExactContentProposal
source current export/disclosure policy = Permit
target capability = memory.write/import
H6 classification = compatible
H8 target encryption = available
provenance = complete
downstream obligations = accepted
```

Flow:

```text
source authorized decrypt/read
→ bounded in-process transfer
→ target classification/redaction
→ fresh target DEK encryption
→ local Memory Candidate + source provenance
→ target activation policy/review
→ source disclosure audit
→ target import audit
```

Source wrapped DEK, source KEK reference и source blob path не копируются.

### 18.3 No silent copy

Следующие действия не создают local Memory автоматически:

- search result display;
- Context Pack inclusion;
- model invocation;
- Run export reference;
- conflict detection;
- mount acceptance;
- source revision update;
- target cache miss.

## 19. Downstream obligations

Grant revision может содержать обязательства:

```text
NoRetention
NoModelUse
NoExternalProvider
NoDerivation
NoExactImport
NoExport
ReviewOnSourceChange
ReviewOnRevocation
PurgeImportedContentOnRevocation
PurgeDerivedIndexesOnRevocation
AttributionRequired
PurposeBound
TimeBound
```

Target обязан принять их при mount activation.

### 19.1 NoRetention

Разрешён transient in-memory use. Persistent target cache/index/content copy запрещены.

### 19.2 ReviewOnSourceChange

Изменение source current revision переводит target derived/imported dependents в review queue, но не переписывает их автоматически.

### 19.3 PurgeImportedContentOnRevocation

Это обязательство допускается только при явном target acceptance и capability. Revocation запускает target-side quarantine/purge workflow для tracked imported descendants.

Source не выполняет прямой delete в target workspace.

Если target purge outcome unknown, состояние остаётся `OutcomeUnknown` и проходит reconciliation.

## 20. Revocation и suspension

### 20.1 Suspension

Suspension обратима только новой source lifecycle mutation и target revalidation.

При suspension:

- новые reads запрещены;
- target caches/indexes quarantine;
- active context dependencies invalidируются;
- audit references сохраняются;
- imported independent memories не удаляются автоматически без obligation.

### 20.2 Revocation

Revocation необратима для exact grant identity.

Новый sharing relationship требует нового grant.

Revocation flow:

```text
source revocation authorized
→ grant state = Revoked
→ source generation increment
→ durable revocation event/outbox
→ target mount = Revoked/Stale
→ read/context use deny
→ cache/index quarantine
→ active Run dependency invalidation
→ downstream obligation jobs
→ source/target audit receipts
```

### 20.3 Delivery ambiguity

Если source committed revoke, но target receipt lost, source truth остаётся authoritative. Target при следующем access проверяет source lifecycle и fail-closed.

Target не продолжает read на основании последнего известного active state при source недоступности, если grant policy не разрешает pinned offline mode.

## 21. Pinned offline mode

Offline use допускается только для exact `ExplicitRevisionSet` и explicit content retention/index permission.

```text
PinnedOfflineMount
├── exact revision manifest
├── exact content hashes
├── local encrypted representations
├── valid_until
├── source disclosure proof
├── target acceptance proof
├── revocation check policy
└── downstream obligations
```

Default offline mode запрещён.

Pinned content:

- не обновляется автоматически;
- всегда показывает snapshot time;
- не обозначается current source memory;
- прекращает use при expiry;
- может требовать periodic revocation check;
- purge/revoke obligations выполняются target-side;
- не разрешает transitive sharing.

## 22. Source deletion, purge и erasure

### 22.1 Soft delete/status change

Source status change немедленно влияет на eligibility. Memory вне `allowed_statuses` исчезает из mount retrieval.

### 22.2 Hard purge

Source hard purge:

- уничтожает source content по owning lifecycle;
- сохраняет content-free tombstone/audit по policy;
- increment source generation;
- уведомляет active mounts;
- invalidates target cache/index/context;
- не позволяет reconstruct content из hash.

### 22.3 Cryptographic erasure

При erasure source content:

- target live mount теряет доступ;
- target derived indexes и retained representations входят в downstream cleanup scope;
- target exact imported Memory обрабатывается согласно accepted obligation;
- source erasure не считается завершённой для downstream copies, если policy обещает cascade и receipt отсутствует;
- результат может быть `SourceErasedDownstreamPending`;
- неизвестный target purge result не трактуется как success.

### 22.4 Historical provenance

Content-free `SharedMemoryRef`, source hash, grant revision и audit fact могут сохраняться, если retention policy разрешает. Они не позволяют восстановить bytes.

## 23. Relations и graph traversal

Mounted relation traversal ограничивается grant selection и operation permission.

Правила:

- relation edge не даёт access target node;
- foreign graph не merge с local graph физически;
- every returned node has `SharedMemoryRef`;
- max depth и node count bounded;
- source relation type сохраняется;
- target может создать local relation только к local proxy/provenance reference, не cross-workspace FK;
- path, содержащий revoked node, не возвращает скрытый content;
- relation expansion не обходит classification filters.

## 24. Transitive sharing запрещён

Target не может поделиться mounted Memory с workspace C.

Запрещено:

```text
workspace A → grant → workspace B
workspace B → re-share mounted ref → workspace C
```

Для C source workspace A создаёт отдельный grant.

Даже если target B импортировал exact content как independent local Memory, дальнейшее sharing зависит от accepted downstream obligations и target ownership policy; provenance source A сохраняется.

Mount-of-mount не поддерживается.

## 25. Model и tool use

### 25.1 Model use

Для передачи mounted content модели требуются:

- `UseInModelContext` source permission;
- source provider-use policy;
- target provider-use policy;
- compatible classification ceiling;
- provider locality;
- exact model/provider revision;
- target Run capability;
- target capture/retention policy;
- bounded Context Pack.

Source может разрешить local model и запретить remote model.

### 25.2 Tool use

Mounted content остаётся data, а не instruction. Tool arguments на основе mounted memory проходят обычную H2 policy, validation и risk checks.

Grant не разрешает external commitment.

### 25.3 Prompt injection

Source content:

- маркируется foreign/untrusted;
- не входит system/developer authority channel;
- не изменяет capability;
- не меняет policy;
- не выполняет embedded commands;
- проходит H6 sanitization/content boundaries.

## 26. Audit и observability

Source audit фиксирует:

- grant create/revise/suspend/revoke/expire;
- target workspace identity;
- selection/category;
- content mode;
- operation class;
- disclosure count/bytes;
- source policy decision;
- exact grant revision;
- disclosure outcome;
- downstream obligation status.

Target audit фиксирует:

- mount inspect/accept/suspend/remove;
- query/use operation;
- target principal/Run;
- source/grant/mount refs;
- target policy decision;
- context/model/export/import use;
- cache/index creation/deletion;
- derivation/import proposal;
- revoke/purge receipt.

Audit не хранит raw secret content или unrestricted query text.

Метрики:

- active grants/mounts;
- reads and denied reads;
- bytes/tokens disclosed;
- stale mount rate;
- revocation propagation latency;
- cache/index cleanup latency;
- derivation/import counts;
- policy denial categories;
- source unavailable outcomes;
- active Run invalidations;
- downstream purge pending/unknown.

## 27. Event contracts

Стабильные event kinds должны следовать durable event schema ADR.

Предлагаемые domain facts:

```text
memory_share.grant_created
memory_share.grant_revision_created
memory_share.grant_pending_acceptance
memory_share.grant_activated
memory_share.grant_suspended
memory_share.grant_revoked
memory_share.grant_expired
memory_share.mount_accepted
memory_share.mount_activated
memory_share.mount_stale
memory_share.mount_suspended
memory_share.mount_revoked
memory_share.read_disclosed
memory_share.read_denied
memory_share.context_use_recorded
memory_share.model_use_recorded
memory_share.derivation_proposed
memory_share.import_proposed
memory_share.import_committed
memory_share.cache_quarantined
memory_share.index_purge_requested
memory_share.index_purged
memory_share.downstream_purge_unknown
```

Event payload не содержит unrestricted memory content.

## 28. Transactional boundaries

### 28.1 Grant mutation

Source transaction атомарно фиксирует:

- grant lifecycle/revision;
- source policy decision ref;
- authoritative event;
- outbox notification intent.

### 28.2 Mount acceptance

Target transaction фиксирует:

- mount identity;
- accepted exact grant revision/hash;
- target policy decision;
- downstream obligations;
- event/outbox.

Activation требует подтверждения, что source revision всё ещё accept-ready.

### 28.3 Disclosure

Read disclosure не обязана сохранять content, но durable accounting/audit receipt фиксируется согласно policy.

### 28.4 Import

Import transaction в target создаёт:

- local Memory Candidate;
- first MemoryRevision;
- MemorySource referencing `SharedMemoryRef`;
- Derivation при derived operation;
- classification/encryption metadata;
- event/outbox;
- idempotency record.

Source disclosure audit связывается correlation ID.

## 29. Failure behavior

### 29.1 Source unavailable

Default:

```text
MountRead = UnavailableFailClosed
```

Stale content не выдаётся.

Pinned offline mode работает только при explicit contract.

### 29.2 Unknown grant revision

Deny. Target не интерпретирует unknown revision как последнюю известную.

### 29.3 Source policy changed

Mount становится stale до revalidation, если effective permissions могут измениться.

### 29.4 Target policy changed

Target немедленно применяет более строгую policy. Расширение target policy не расширяет source grant.

### 29.5 Partial cache cleanup

Cache/index переводится в non-queryable quarantine до завершения. Cleanup retry безопасен и идемпотентен.

### 29.6 Revoke receipt missing

Future read выполняет source lifecycle check и deny. Receipt reconciliation продолжается отдельно.

### 29.7 Import completion unknown

Проверяется target idempotency key, source ref и local provenance. Второй import blind не выполняется.

### 29.8 Source revision changed during read

Read возвращает exact revision actually disclosed либо conflict/retry-safe result. Нельзя смешивать metadata одной revision с content другой.

## 30. Idempotency

Внешние commands требуют idempotency key:

```text
CreateMemoryShareGrant
CreateGrantRevision
SubmitGrantForAcceptance
AcceptMemoryMount
SuspendGrant
RevokeGrant
SuspendMount
RemoveMount
ProposeMemoryImport
CommitMemoryImport
```

Idempotency scope включает source/target workspace, operation, object и canonical input hash.

Повтор не создаёт duplicate grant, mount, import или event.

## 31. Optimistic concurrency

Mutable identities используют expected revision:

- grant state revision;
- mount state revision;
- collection revision;
- import proposal state revision.

Immutable revisions не обновляются.

Concurrent source revoke и target read разрешаются в пользу revoke boundary: disclosure commit обязан доказать active grant revision at authorization point. Если это невозможно, read deny/retry без content.

## 32. Encryption boundary

### 32.1 Live read

Source workspace:

- authorizes read;
- decrypts through H8 bounded operation;
- returns only allowed content DTO;
- zeroizes transient plaintext according to implementation contract.

Target не получает source KEK, wrapped DEK или blob path.

### 32.2 Target persistence

Любое разрешённое persistent target representation:

- создаётся fresh target DEK;
- связывается с target workspace/object/purpose AEAD context;
- не использует source deterministic hash для cross-workspace dedup;
- имеет mount/grant/source provenance;
- входит в revoke/purge cleanup tracking.

### 32.3 Key outage

Нет plaintext fallback. Live read fail-closed. Optional target indexing не создаётся.

## 33. Signed Run export integration

Run export может включать mounted content только если на snapshot boundary одновременно разрешены:

- `IncludeInRunExport`;
- source current export policy;
- target export policy;
- exact content/classification mode;
- active grant/mount;
- downstream obligations;
- H2 authorization.

Export manifest фиксирует:

```text
source_workspace_id
shared_ref
grant_revision_id
mount_id
source_disclosure_decision_id
target_export_decision_id
content_included | reference_only | tombstone
```

Import Run export:

- не создаёт grant;
- не создаёт mount;
- не активирует foreign Memory;
- сохраняет source provenance namespace;
- рассматривает included bytes как inert bundle content;
- требует отдельной local import operation для Memory creation.

## 34. Administrative surfaces

H11 может предоставить:

```text
POST /admin/v1/memory-share-grants
POST /admin/v1/memory-share-grants/{id}/revisions
POST /admin/v1/memory-share-grants/{id}/submit
POST /admin/v1/memory-share-grants/{id}/suspend
POST /admin/v1/memory-share-grants/{id}/revoke
GET  /admin/v1/memory-share-grants/{id}
GET  /admin/v1/memory-share-invitations
POST /admin/v1/memory-mounts/{grant-id}/accept
POST /admin/v1/memory-mounts/{id}/suspend
DELETE /admin/v1/memory-mounts/{id}
GET  /admin/v1/memory-mounts/{id}
POST /v1/memories/search-with-mounts
POST /v1/memory-import-proposals
```

Названия являются design-level surface, не разрешением на implementation.

MCP agent-facing surface по умолчанию:

- может искать через уже активные mounts в пределах capability;
- не может создавать/принимать/revoke grants;
- не может импортировать exact content без отдельной governed command;
- всегда получает origin/provenance labels.

## 35. Safe UI requirements

UI показывает:

- source workspace;
- target workspace;
- active grant revision;
- purpose;
- expiry;
- content/classification ceiling;
- разрешённые operations;
- model/provider use;
- indexing/retention;
- downstream obligations;
- current lifecycle;
- last successful source check;
- affected active Runs/imports/indexes.

UI не показывает secret references/keys и не маскирует revoke как обычный toggle.

Перед acceptance/revoke/purge UI требует review exact scope и consequences.

## 36. Retention

Grant/mount metadata и content-free audit могут жить дольше content.

Retention classes разделяются:

```text
GrantLifecycleMetadata
DisclosureAudit
TargetUseAudit
TransientQueryData
TargetDerivedIndex
ImportedMemoryContent
DownstreamPurgeEvidence
```

Raw target query и disclosed content не сохраняются только ради diagnostics.

## 37. Acceptance scenarios

### Scenario A — read-only procedures library

1. Workspace A создаёт explicit collection процедур.
2. Grant разрешает `StructuredOnly`, deterministic context, без model use/export/derivation.
3. Workspace B принимает mount.
4. B ищет процедуры federated retrieval.
5. Source rows читаются под A RLS.
6. Results помечены Mounted.
7. Local Memory B не создаётся.

Expected: read succeeds within limits; no local copy or embedding.

### Scenario B — guessed UUID

Target передаёт source `MemoryId` вне selection.

Expected: not found/denied без раскрытия existence.

### Scenario C — revoke during active Run

1. Run B использует mounted memory in context.
2. A revokes grant.
3. Context dependency invalidates.
4. Future invocation pauses/rebuilds.
5. Historical invocation remains audited.

Expected: no stale retry with foreign content.

### Scenario D — foreign fact conflicts with local fact

Expected: conflict proposal created; mounted fact does not supersede local memory automatically.

### Scenario E — exact import

1. Grant explicitly permits import.
2. B proposes import.
3. Source disclosure and target write authorized.
4. Content re-encrypted fresh B DEK.
5. Local Memory starts Candidate with full provenance.

Expected: no source key/path copied; no automatic Active status unless policy permits.

### Scenario F — source purge

Expected: live result disappears, caches/indexes quarantine, context invalidates, tombstone remains.

### Scenario G — transitive share attempt

B tries to grant mounted A memory to C.

Expected: denied. A must issue direct grant to C.

### Scenario H — remote model prohibited

Grant allows read but `NoExternalProvider`.

Expected: UI/search allowed; remote model context denied; local deterministic use may continue.

### Scenario I — target-side embedding without permission

Expected: denied; no embedding job or persistent target index.

### Scenario J — stale revision acceptance

Source creates revision 2 while target accepts revision 1.

Expected: conflict; target must inspect/accept current exact revision.

### Scenario K — source unavailable

Expected: fail-closed for live mount; stale cache not returned.

### Scenario L — signed export

Export requests mounted bytes without `IncludeInRunExport`.

Expected: reference/tombstone only or deny according to profile.

### Scenario M — target policy becomes stricter

Expected: immediate target-side reduction/deny; source grant unchanged.

### Scenario N — erasure cascade unknown

Source requires purge imported descendants; target purge result ambiguous.

Expected: downstream status `OutcomeUnknown`; source does not claim complete cascade.

### Scenario O — prompt injection in shared procedure

Expected: content treated as untrusted data; no policy/capability elevation.

## 38. Verification properties for future implementation

Future implementation plan must include tests proving:

1. cross-workspace UUID alone never grants access;
2. scope `global` remains workspace-local;
3. target cannot mutate source Memory;
4. source and target policies are both enforced;
5. stale/revoked/expired grant denies content;
6. target cannot widen grant;
7. mount-of-mount is rejected;
8. read-only search creates no local Memory;
9. no target embedding by default;
10. target persistent representation uses fresh target DEK;
11. source KEK/DEK never appear in target rows/logs/export;
12. foreign confidence is not copied as local confidence automatically;
13. mounted memory cannot supersede local memory automatically;
14. revoke invalidates cache/context/active Run dependency;
15. source purge removes queryability before async physical cleanup completes;
16. import creates provenance and obeys write policy;
17. duplicate idempotency key creates no duplicate grant/mount/import;
18. concurrent revoke/read resolves fail-closed;
19. unknown schema/revision fails closed;
20. Run export does not recreate grant/mount on import;
21. Restricted/Secret policy behaves correctly;
22. provider locality is enforced;
23. audit is present on source and target;
24. downstream purge unknown is not reported as success;
25. graph traversal cannot escape selection.

## 39. Required future implementation boundaries

После письменного утверждения отдельный implementation plan должен определить:

- domain IDs и value types;
- application ports/commands;
- source/target handshake;
- persistence ownership без cross-workspace FK;
- migrations после текущего locked sequence;
- RLS-safe federated query adapter;
- outbox/reconciliation;
- H6 context/index/cache integration;
- H8 re-encryption integration;
- H10 audit and metrics;
- H11 admin/public/SDK surfaces;
- deterministic tests;
- rollout feature flag disabled by default;
- migration/backfill policy;
- security review gate;
- load and revocation-latency tests.

Этот design не выбирает migration numbers и не разрешает создание implementation branch.

## 40. Security invariants

1. Workspace isolation остаётся default.
2. `global` не означает cross-workspace.
3. Grant всегда связывает один source и один exact target.
4. Target acceptance обязательна.
5. Grant revision неизменяема.
6. Mount read-only по умолчанию.
7. UUID/hash/reference не предоставляют permission.
8. Source и target policy применяются независимо.
9. RLS не отключается ради federated retrieval.
10. Raw cross-workspace SQL запрещён.
11. Mounted content не является local authority.
12. Mounted content не является instruction authority.
13. Mounted content не supersede local memory автоматически.
14. Mounted memory не re-share transitively.
15. Mount-of-mount запрещён.
16. Target indexing disabled по умолчанию.
17. Persistent target derivative использует fresh target DEK.
18. Source keys не покидают source/H8 boundary.
19. Revoked/expired/stale mount не выдаёт content.
20. Source purge/erasure invalidates all system derivatives.
21. Historical audit не переписывается.
22. Active Run не продолжает stale content use автоматически.
23. Export требует отдельное source permission.
24. Import создаёт новую local authority и provenance, а не меняет source.
25. Downstream purge result должен быть правдивым.

## 41. Итоговое решение

Vestrace поддерживает cross-workspace memory sharing только через явную пару:

```text
Source MemoryShareGrant
        ↓ exact immutable revision
Target MemoryMount
```

Default behavior:

```text
read-only
live authorized query
no target copy
no target embedding
no model use unless explicit
no export unless explicit
no derivation/import unless explicit
no transitive sharing
fail-closed on stale/revoke/source outage
```

Это сохраняет базовую v0.1 модель памяти и RLS, добавляя управляемый, двусторонний и полностью provenance-aware канал disclosure вместо скрытого расширения workspace scope.
