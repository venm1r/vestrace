# Vestrace — signed portable Run export design

**Статус:** предложено для письменного утверждения  
**Дата:** 2026-08-01  
**Репозиторий:** `venm1r/vestrace`  
**Связанный boundary:** `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`  
**Совместимость событий:** `docs/superpowers/specs/2026-07-31-vestrace-event-schema-compatibility-adr.md`  
**Владельцы контрактов:** H1, H2, H5, H6, H8, H10 и H11

> Этот документ является documentation-only design. Он не разрешает менять Rust-код, создавать миграции, генерировать реальные экспортные архивы, выпускать signing keys, добавлять public endpoints или импортировать данные.

## 1. Назначение

Документ определяет переносимый, подписанный и проверяемый формат `vestrace-run-export` для одного `AgentRun` и явно выбранных связанных данных.

Формат предназначен для:

- аудита;
- переноса между совместимыми self-hosted deployments;
- поддержки и диагностики;
- офлайн-инспекции;
- подготовки evaluation fixtures;
- регуляторного или внутреннего evidence package;
- долговременного сохранения доказательной истории Run в пределах policy.

Export является **инертным доказательным bundle**. Он переносит проверяемые факты и разрешённые данные, но не переносит доменную authority.

## 2. Нормативный принцип

> **Подпись Run export подтверждает целостность точного manifest и включённых bytes. Она не подтверждает истинность результата модели, не выдаёт разрешения, не восстанавливает секреты и не разрешает автоматически продолжить или повторить Run.**

## 3. Явно не входит

`vestrace-run-export` не является:

- дампом PostgreSQL;
- резервной копией deployment;
- backup credential backend;
- контейнером с usable secrets;
- способом переноса bearer tokens, refresh tokens, private keys или passwords;
- автоматическим replay package;
- механизмом запуска, resume или активации Run;
- механизмом активации memory candidates;
- переносом leases, очереди работ или live worker ownership;
- способом обхода H2 policy;
- доказательством доверенности signer без внешнего trust anchor;
- форматом полного клонирования workspace;
- универсальным архивом всех связанных conversations;
- гарантией полной воспроизводимости, если policy запрещает необходимые captures;
- заменой H1 logical replay или H10 safe replay;
- механизмом cross-workspace sharing;
- форматом incremental database replication.

## 4. Владение данными

Export не меняет существующие authority boundaries.

| Область | Владелец | Роль в export |
|---|---|---|
| Run lifecycle, journal, checkpoint boundary | H1 | предоставляет точную snapshot boundary и immutable references |
| Authorization, approvals, budgets | H2 | разрешает exact export scope и формирует безопасные decision references |
| Plans, SubRuns, handoffs | H5 | предоставляет immutable plan/delegation references и разрешённые manifests |
| Context, Artifacts, evidence, purge | H6 | классифицирует, отбирает, сканирует, экспортирует bytes и tombstones |
| Signing credential | H8 | выдаёт ограниченный signing lease без раскрытия private key |
| Audit integrity и evaluation | H10 | предоставляет audit proof и принимает inert bundle как evaluation input после проверки |
| HTTP, CLI, MCP, SDK, UI | H11 | предоставляет adapters над одним application contract |

Export builder не становится владельцем Run, Artifact, policy, credential или audit state.

## 5. Модель export operation

### 5.1 Команда

Концептуальная команда:

```text
CreateRunExport
├── workspace_id
├── run_id
├── expected_run_version?
├── snapshot_boundary
├── export_profile
├── artifact_selection
├── context_selection
├── audit_proof_selection
├── destination_classification
├── requested_format_version
├── authorization_reference
└── idempotency_key
```

### 5.2 Состояния

```text
Requested
→ Authorizing
→ Snapshotting
→ Building
→ Validating
→ Signing
→ Succeeded

Terminal alternatives:
Denied
Failed
Cancelled
```

Создание локального bundle и его доставка во внешнее место являются разными операциями.

- `Succeeded` означает, что проверенный bundle создан как H6 Artifact revision.
- Download, copy, upload или внешняя доставка требуют отдельного H2/H6 authorization и отдельного audit fact.
- Неоднозначный исход внешней доставки не изменяет целостность уже созданного локального bundle.

### 5.3 Идемпотентность

Одинаковая команда с тем же `idempotency_key`, snapshot boundary, scope и policy revisions должна ссылаться на тот же logical export request.

Повторная сборка может создать новый export Artifact только если изменился хотя бы один из следующих элементов:

- snapshot boundary;
- export format version;
- selection manifest;
- policy decision;
- redaction/purge state;
- schema set;
- signer key revision;
- builder revision.

## 6. Snapshot boundary

Export фиксирует точную H1 границу:

```text
RunExportSnapshotBoundary
├── run_id
├── run_version
├── last_included_run_sequence
├── active_plan_revision_id?
├── latest_included_checkpoint_id?
├── captured_at
└── source_snapshot_hash
```

### 6.1 Terminal Run

Для terminal Run export включает journal до terminal event, если policy не исключает конкретные payload bytes.

### 6.2 Active Run

Active Run может экспортироваться только как consistent point-in-time snapshot.

Export обязан:

1. прочитать exact Run version;
2. зафиксировать последнюю включённую sequence;
3. включить только checkpoints, plan revisions, operations и Artifacts, существовавшие к этой границе;
4. не утверждать, что Run завершён;
5. обозначить `run_lifecycle_at_export` и `snapshot_is_terminal=false`.

Export не требует остановки Run, если H1 может гарантировать consistent read. Если гарантировать границу нельзя, операция завершается безопасной ошибкой, а не собирает смешанное состояние.

### 6.3 SubRuns

Parent export не включает SubRun автоматически.

Возможные режимы:

```text
ReferencesOnly
SelectedSubRuns
FullAuthorizedClosure
```

Каждый включённый SubRun имеет собственную snapshot boundary и отдельное H2/H6 решение. Отсутствующий SubRun обозначается reference или tombstone, а не неявно копируется.

## 7. Логическая структура bundle

Нормативная структура:

```text
vestrace-run-export/
├── manifest.json
├── run.json
├── events.ndjson
├── plan-revisions/
│   └── <revision-id>.json
├── checkpoints/
│   └── <checkpoint-id>.json
├── policy-and-budget-references/
│   ├── policy-decisions.ndjson
│   ├── approvals.ndjson
│   └── accounting-manifest.json
├── context-manifests/
│   └── <context-snapshot-id>.json
├── artifact-manifest.json
├── artifacts/
│   └── <content-hash-or-bundle-path>
├── audit-chain-proof/
│   ├── proof-manifest.json
│   ├── records.ndjson
│   └── checkpoints.json
├── schemas/
│   ├── export/
│   └── events/
├── tombstones/
│   └── <tombstone-id>.json
├── checksums.sha256
└── signature.json
```

Логическая структура не требует одного конкретного archive container.

Допустимые transport containers определяются H11 adapters, например tar-compatible или ZIP-compatible representation. Целостность относится к логическим entries и их canonical paths, а не к container metadata, compression level или byte offsets.

## 8. Required и optional entries

### 8.1 Обязательные entries

Каждый bundle обязан содержать:

```text
manifest.json
run.json
events.ndjson
artifact-manifest.json
schemas/export/*
checksums.sha256
signature.json
```

Также обязательны все event schemas, необходимые для интерпретации включённых `events.ndjson`, если manifest не ссылается на полностью идентичный immutable schema package, доступность которого гарантирована import policy.

Portable offline profile всегда включает необходимые schemas.

### 8.2 Условно обязательные entries

Entry становится обязательным, если manifest заявляет соответствующее содержимое:

- plan revision file для каждой включённой plan revision;
- checkpoint file для каждой включённой checkpoint reference;
- context manifest для каждого included context snapshot;
- artifact bytes для каждой записи `content_included=true`;
- tombstone для каждого заявленного purged/withheld object;
- audit records/checkpoints для заявленного audit proof range.

### 8.3 Optional entries

Policy может исключить:

- checkpoint payload;
- context payload или excerpts;
- Artifact bytes;
- selected audit records;
- policy explanation details;
- approval rationale;
- nonessential operational diagnostics;
- public conversation content.

Исключение не должно быть тихим. Manifest обязан указывать omission category и причину, не раскрывающую запрещённые данные.

## 9. `manifest.json`

Концептуальная модель:

```text
RunExportManifest
├── format
│   ├── name = "vestrace-run-export"
│   ├── major
│   ├── minor
│   └── manifest_schema_version
├── export_id
├── created_at
├── builder_revision
├── source
│   ├── deployment_instance_id
│   ├── workspace_id
│   ├── run_id
│   └── source_product_version
├── snapshot_boundary
├── run_lifecycle_at_export
├── selection
├── included_entries[]
├── omitted_entries[]
├── tombstones[]
├── event_schema_inventory[]
├── artifact_inventory_summary
├── audit_proof_summary
├── classification_summary
├── redaction_summary
├── trust_metadata
├── canonicalization
├── checksum_algorithm
├── signature_algorithm
└── root_hash
```

### 9.1 `export_id`

- UUIDv7;
- идентифицирует один logical bundle;
- не становится local authority при import;
- не используется как единственный idempotency key.

### 9.2 Source identity

`deployment_instance_id`, `workspace_id` и `run_id` являются source identifiers.

После cross-deployment import они сохраняются в provenance namespace и не должны автоматически занимать local IDs.

### 9.3 Selection

Manifest фиксирует exact selection:

```text
selection
├── journal = Full | BoundedRange
├── plans = ReferencesOnly | Included
├── checkpoints = None | Selected | Latest | AllAuthorized
├── context = MetadataOnly | SelectedRedacted | SelectedFull
├── artifacts = ManifestOnly | Selected | AllAuthorized
├── audit_proof = None | SelectedRange | RunRelevantRange
└── subruns = ReferencesOnly | Selected | FullAuthorizedClosure
```

Значение `Full` всегда означает «полный в пределах точной policy и snapshot boundary», а не «игнорировать policy».

## 10. `run.json`

`run.json` содержит bounded, versioned representation H1 Run state на snapshot boundary.

Он может включать:

- objective в разрешённой форме;
- exact Run status;
- run version;
- active plan revision reference;
- success criteria references;
- terminal outcome reference;
- budget summary;
- relevant actor/profile revision references;
- creation/start/finish timestamps;
- source workspace and principal references в безопасной форме.

Он не включает:

- worker lease token;
- live queue ownership;
- continuation token;
- usable approval token;
- credential handle;
- secret reference, пригодную для использования;
- backend connection string;
- internal storage path;
- private scratchpad или hidden reasoning.

## 11. `events.ndjson`

### 11.1 Exact persisted identity

Каждая строка представляет один включённый durable event в его exact stable kind и schema version.

Export сохраняет:

- event ID;
- stable event kind;
- original schema version;
- owning sequence/cursor;
- recorded/occurred timestamps;
- safe authority/reference fields;
- exact included payload bytes либо explicit payload omission descriptor;
- original payload hash;
- export representation hash.

### 11.2 Upcasting

Export не заменяет historical event его upcast representation.

При необходимости bundle может дополнительно включить derived compatibility view, но оно:

- имеет отдельный path и schema;
- явно помечено как derived;
- ссылается на original event ID/hash;
- не участвует вместо original event в audit proof;
- не меняет historical sequence.

### 11.3 Payload omission

Если event payload содержит запрещённое содержимое, export включает safe envelope и descriptor:

```text
payload_state = OmittedByPolicy | Purged | Unavailable | Quarantined
payload_hash = <historical hash when retention permits>
omission_reference = <tombstone-id>
```

Export не подставляет пустой объект, который можно ошибочно принять за original payload.

## 12. Plans, checkpoints и context manifests

### 12.1 Plan revisions

Каждая включённая H5 plan revision неизменяема и содержит:

- revision ID;
- canonical plan hash;
- parent revision reference;
- typed steps и dependencies;
- required capabilities/tools как references;
- success criteria references;
- creation provenance;
- redaction/omission metadata.

Completed history не переписывается под последнюю plan revision.

### 12.2 Checkpoints

Checkpoint export является optional и policy-controlled.

Включённый checkpoint:

- соответствует exact H1 run version/sequence;
- не содержит usable leases;
- не содержит resumable bearer/continuation secret;
- не разрешает автоматический resume;
- использует safe references вместо backend paths;
- помечается `inspection_only=true`.

Import checkpoint не создаёт H1 `RunCheckpoint` автоматически.

### 12.3 Context manifests

Context manifest по умолчанию содержит:

- ContextSnapshot ID;
- rendered hash;
- exact source/revision references;
- token accounting;
- capture mode/profile revision;
- omission/redaction reasons;
- classification summary;
- Artifact references для разрешённых captures.

Hidden reasoning и provider reasoning-token content не включаются ни при каком export profile.

## 13. Policy, approval и budget references

Export переносит доказательные references, а не активную authority.

Допустимы:

- policy bundle revision IDs/hashes;
- decision ID;
- normalized decision result;
- obligations summary;
- exact operation hash;
- approval state и validity interval на момент операции;
- accounting entries и budget totals;
- content-free denial/expiry facts.

Запрещены:

- reusable authorization ticket;
- bearer approval token;
- policy signing private key;
- unrestricted policy source, если classification это запрещает;
- local principal session;
- claim, что approval остаётся действующим на target deployment.

Все imported approvals являются historical evidence only.

## 14. Artifact manifest и bytes

### 14.1 Manifest record

```text
ExportedArtifactRecord
├── source_artifact_id
├── source_revision_id
├── kind
├── media_type
├── byte_size
├── content_hash
├── classification
├── producer_reference
├── provenance_hash
├── content_state
├── bundle_path?
├── inspection_state
├── redaction_state
├── retention_state
└── tombstone_reference?
```

### 14.2 Content states

```text
Included
ManifestOnly
RedactedRepresentationIncluded
Purged
WithheldByPolicy
Quarantined
Unavailable
```

### 14.3 Byte inclusion

Artifact bytes включаются только после:

1. H2 authorization;
2. H6 classification/export policy;
3. current lifecycle check;
4. secret scan;
5. quarantine/inspection check;
6. destination classification check;
7. exact content hash verification.

### 14.4 Paths

Bundle никогда не содержит source backend paths, bucket keys, host paths или reusable presigned URLs.

`bundle_path` является локальным относительным path внутри export и не предоставляет permission вне него.

### 14.5 Deduplication

Одинаковые bytes могут храниться один раз внутри bundle по content hash, если:

- все referencing records разрешают inclusion;
- classification совместима;
- manifest сохраняет все logical references;
- dedup не раскрывает cross-workspace existence.

Cross-workspace dedup inference запрещён.

## 15. Tombstones и purged content

Tombstone используется, когда объект существовал в source history, но содержимое отсутствует или не может быть экспортировано.

```text
ExportTombstone
├── tombstone_id
├── object_kind
├── source_reference
├── historical_content_hash?
├── state
├── reason_category
├── source_fact_reference?
├── effective_at?
└── explanation
```

Допустимые states:

```text
Purged
CryptographicallyErased
WithheldByPolicy
NotCaptured
Unavailable
Quarantined
```

Tombstone:

- не реконструирует содержимое;
- не хранит secret value;
- не утверждает, что отсутствие является ошибкой;
- позволяет объяснить неполную воспроизводимость;
- входит в checksums и signature scope.

## 16. Audit-chain proof

### 16.1 Назначение

Audit proof позволяет проверить, что включённые H10 audit records согласованы с заявленным source chain range.

Он не доказывает, что source deployment не имел других незаявленных записей вне диапазона.

### 16.2 Proof manifest

```text
AuditChainProofManifest
├── audit_stream_id
├── first_sequence
├── last_sequence
├── first_previous_hash
├── last_record_hash
├── included_record_count
├── checkpoint_references[]
├── signature_verification_material[]
└── completeness_statement
```

### 16.3 Completeness statements

```text
ExactContiguousRange
RunRelevantFilteredView
CheckpointOnly
Unavailable
```

Filtered view не должен называться полным chain range.

### 16.4 Existing signatures

Source audit checkpoint signatures сохраняются отдельно от export signature.

- Audit signature подтверждает source audit chain checkpoint.
- Export signature подтверждает exact bundle contents.
- Одна подпись не заменяет другую.

## 17. Schema package

Bundle включает:

- schema export manifest;
- schemas для `manifest.json`, `run.json`, tombstones и других export records;
- event schemas для всех включённых `(stable_event_kind, schema_version)`;
- schema hashes;
- compatibility metadata;
- optional pure upcaster metadata без исполняемого произвольного кода.

Bundle не включает и не активирует executable upcaster plugin.

Unknown schema может быть сохранена для inert inspection как bytes, но semantic import/replay остаётся blocked.

## 18. Canonical paths и container safety

Каждый logical entry path обязан:

- быть относительным;
- использовать `/` как separator;
- быть UTF-8;
- находиться в Unicode normalization form, определённой format version;
- не содержать `.` или `..` segments;
- не начинаться с `/`;
- не содержать NUL;
- не дублировать другой path после normalization/case policy;
- не быть symlink, hard link, device, socket или executable container directive.

Container reader обязан ограничивать:

- общий compressed/uncompressed размер;
- число entries;
- глубину path;
- размер одного entry;
- compression ratio;
- duplicate paths;
- nested archives;
- parsing time и memory.

Container timestamps, ownership bits, compression metadata и entry order не участвуют в semantic signature.

## 19. Canonical serialization

### 19.1 JSON

Каждый JSON document использует Vestrace-owned canonical JSON profile:

- UTF-8;
- deterministic object-key ordering;
- отсутствие insignificant whitespace;
- deterministic escaping;
- целые числа в canonical decimal form;
- floating-point values не используются в authoritative hashes;
- timestamps в одном нормализованном UTC format;
- duplicate object keys запрещены;
- invalid Unicode запрещён.

Точная canonicalization revision фиксируется в manifest.

### 19.2 NDJSON

- одна canonical JSON record на строку;
- line ending `LF`;
- no blank lines;
- deterministic order по owning sequence/cursor;
- финальный `LF` обязателен;
- record hash может быть проверен независимо.

### 19.3 Binary Artifacts

Binary bytes хешируются без преобразования.

Media normalization, text newline conversion или recompression создают отдельную H6 representation с собственным hash.

## 20. Checksums

`checksums.sha256` содержит все logical entries, кроме:

```text
checksums.sha256
signature.json
```

Формат строки:

```text
<lowercase-sha256-hex><two-spaces><canonical-path>\n
```

Правила:

- paths отсортированы по bytewise order canonical UTF-8;
- каждая entry встречается ровно один раз;
- manifest включён;
- Artifact bytes включены;
- tombstones и schemas включены;
- отсутствующая заявленная entry является ошибкой;
- лишняя незаявленная entry является ошибкой, если format не допускает explicit extension namespace.

## 21. Root hash и signature scope

### 21.1 Root hash

После создания `checksums.sha256` builder вычисляет:

```text
root_hash = SHA-256(
    domain_separator
    || canonical_format_identity
    || SHA-256(manifest.json bytes)
    || SHA-256(checksums.sha256 bytes)
)
```

Где:

```text
domain_separator = "vestrace-run-export-signature\0"
```

Exact encoding полей определяется format major.

### 21.2 Signature

`signature.json` подписывает root hash через H8 signing port.

Reference algorithm первой версии:

```text
Ed25519
```

Signature record:

```text
RunExportSignature
├── signature_schema_version
├── algorithm
├── signed_root_hash
├── signature_bytes
├── signer_key_id
├── signer_key_revision
├── public_key_fingerprint
├── verification_material?
├── signed_at
├── trust_domain
└── signer_statement
```

### 21.3 Private key boundary

Private signing key:

- не покидает H8 backend;
- не попадает в work item;
- не попадает в PostgreSQL payload;
- не попадает в bundle;
- не передаётся модели;
- используется через bounded one-purpose signing lease.

### 21.4 Integrity и trust

Успешная cryptographic verification доказывает:

- signature соответствует root hash;
- root hash соответствует manifest и checksums;
- checksums соответствуют included entries.

Она не доказывает автоматически:

- что signer доверен import deployment;
- что source policy была правильной;
- что модельный результат истинный;
- что bundle полный за пределами заявленного scope;
- что timestamps подтверждены внешним trusted time source.

Trust определяется отдельно по configured trust anchors, key status, revocation policy и source identity.

## 22. Format versioning

### 22.1 Version model

```text
format.name  = vestrace-run-export
format.major = 1
format.minor = 0
```

- Major изменяется при несовместимой структуре, canonicalization или signature scope.
- Minor изменяется при совместимых optional additions.
- Schema versions отдельных records версионируются независимо.

### 22.2 Unknown major

Unknown major:

- может быть сохранён как quarantined H6 Artifact;
- не проходит semantic import;
- не активируется;
- не используется для replay;
- не интерпретируется как ближайший известный major.

### 22.3 Unknown optional entry

Unknown entry допустима только если:

- находится в versioned extension namespace;
- объявлена в manifest;
- включена в checksums/signature;
- reader policy разрешает inert preservation;
- entry не заявляет authority или executable semantics.

## 23. Authorization

### 23.1 Exact authorization request

H2 request должен включать:

- principal;
- workspace и Run;
- snapshot boundary;
- выбранные event/context/artifact/audit scopes;
- destination classification;
- предполагаемый delivery channel;
- maximum export size;
- format version;
- signer key purpose;
- expiry;
- maximum executions;
- operation hash.

### 23.2 Approval binding

Если требуется approval, оно связывается с exact normalized export operation hash.

Изменение scope, snapshot boundary, Artifact selection, destination, format или signer revision инвалидирует approval.

### 23.3 Deny by default

Отсутствующая или неоднозначная policy приводит к denial.

Export builder не понижает classification, не расширяет selection и не использует cached authorization после expiry/revocation.

## 24. Sensitive data и секреты

Bundle никогда не содержит:

- passwords;
- access/refresh tokens;
- private keys;
- session cookies;
- raw authorization headers;
- database credentials;
- secret backend locators, пригодные для извлечения;
- reusable credential lease;
- presigned storage URLs;
- host paths;
- unrestricted environment dumps;
- hidden chain-of-thought;
- provider reasoning-token content;
- private model scratchpad;
- unbounded external request/response headers.

Logical connection или secret facts могут экспортироваться только как non-usable content-free references, если policy это разрешает.

Перед signing выполняется финальный secret scan по всем eligible textual/binary representations, для которых scanner существует. Scan outcome входит в manifest и audit.

Scan failure не приводит к «подписать как есть». Операция понижает scope, исключает entry либо завершается fail closed согласно policy.

## 25. Export profiles

Начальные profiles:

### 25.1 `AuditMinimal`

- full safe event envelopes;
- payload omissions по policy;
- no Artifact bytes по умолчанию;
- policy/approval/accounting references;
- audit proof;
- schemas и tombstones.

### 25.2 `SupportDiagnostic`

- `AuditMinimal`;
- bounded operational diagnostics;
- selected redacted context manifests;
- selected redacted Artifact representations;
- explicit expiry/retention.

### 25.3 `PortableEvaluation`

- exact journal and schemas;
- selected deterministic fixtures/recorded observations;
- selected redacted context;
- evaluation-relevant Artifacts;
- replayability assessment;
- no effectful credentials or remote bindings.

### 25.4 `MigrationEvidence`

- maximum authorized immutable references/bytes;
- exact source IDs and provenance;
- checkpoints inspection-only;
- no automatic local ID adoption;
- no activation semantics.

Profiles являются presets. Итоговый scope всегда определяется H2/H6 policy и может быть только уже.

## 26. Import pipeline

Import — это bounded verification и inert registration, а не выполнение.

```text
received container bytes
→ quarantine
→ container/path/size validation
→ bounded read of manifest, checksums and signature
→ format-major validation
→ streaming checksum verification
→ root-hash verification
→ cryptographic signature verification
→ signer trust evaluation
→ workspace binding/remap evaluation
→ schema inventory validation
→ classification and secret-scan policy
→ semantic parsing
→ inert ImportedRunBundle registration
→ optional inspection/evaluation staging
```

### 26.1 До signature verification

До проверки signature разрешены только bounded structural operations, необходимые для извлечения и проверки manifest/checksums/signature.

Запрещено:

- десериализовать произвольные event payload в domain commands;
- вызывать upcasters;
- создавать AgentRun;
- создавать memory candidates;
- монтировать Artifact в sandbox;
- вызывать tools/models/connectors;
- доверять source paths или MIME declarations.

### 26.2 После signature verification

Даже после проверки bundle остаётся untrusted external input до завершения:

- trust-anchor evaluation;
- classification policy;
- schema support checks;
- workspace binding;
- malware/content inspection;
- H2 authorization для конкретного import mode.

## 27. Import modes

Разрешённые inert modes:

```text
InspectOnly
SupportCase
EvaluationFixture
MigrationStaging
```

### 27.1 InspectOnly

Создаёт read-only view над verified bundle. Не создаёт local Run state.

### 27.2 SupportCase

Регистрирует bundle как H6 Artifact/evidence, доступный только authorized support scope.

### 27.3 EvaluationFixture

После отдельного H10 validation создаёт immutable evaluation input references. Effectful ports остаются запрещёнными.

### 27.4 MigrationStaging

Создаёт staging records и mapping proposal. Перенос в local domain требует отдельного future migration design и commands.

Ни один mode не:

- запускает Run;
- выполняет resume;
- отправляет notification;
- вызывает tool/model;
- активирует memory;
- применяет policy из bundle;
- активирует approval;
- устанавливает extension;
- импортирует credential bytes.

## 28. Workspace binding и remap

### 28.1 Same-workspace import

Совпадение source workspace ID недостаточно.

Import проверяет:

- configured deployment/workspace trust binding;
- source signer trust;
- expected source deployment identity;
- current H2 policy;
- classification compatibility.

### 28.2 Cross-workspace import

Cross-workspace import требует explicit approved remap:

```text
RunExportWorkspaceRemap
├── source_deployment_id
├── source_workspace_id
├── target_workspace_id
├── allowed_object_kinds
├── classification_mapping
├── principal_mapping?
├── retention_mapping
├── valid_until
├── maximum_imports
└── authorization_reference
```

Без approved remap bundle доступен только в quarantine/inspection scope либо import отклоняется.

### 28.3 ID handling

Source IDs сохраняются как provenance.

Local persistent objects, если future migration design разрешит их создание, получают новые local IDs и explicit mapping. Silent ID adoption запрещён.

## 29. Unknown и unsupported schemas

### 29.1 Integrity verification

Checksum/signature verification может завершиться успешно даже при unknown event schema.

Это означает только, что bytes не изменены относительно подписанного bundle.

### 29.2 Semantic handling

Unknown или unsupported schema:

- блокирует semantic interpretation соответствующего record;
- блокирует replay/resume/reconciliation;
- помечает bundle `PartiallyInterpretable` или `Unsupported`;
- сохраняет exact bytes в quarantine, если policy разрешает;
- не вызывает best-effort mapping к последней известной версии.

### 29.3 Upcasters

Используются только repository-owned pure upcasters, уже установленные и разрешённые target deployment.

Bundle не может поставлять автоматически исполняемый upcaster.

## 30. Re-export и provenance

Imported bundle может быть вложен в новый support/evidence export только как immutable source Artifact.

Новый export:

- сохраняет original manifest/checksums/signature;
- добавляет wrapper provenance;
- не заменяет original signature;
- не утверждает, что target deployment является original signer;
- подписывает собственный wrapper scope отдельной подписью.

## 31. Failure semantics

### 31.1 Authorization denied

Bundle не создаётся. Создаётся content-free denial audit fact.

### 31.2 Snapshot changed

При `expected_run_version` mismatch операция завершается version conflict. Builder не молча экспортирует новую версию.

### 31.3 Artifact changed or unavailable

Immutable content hash mismatch является integrity failure.

Policy определяет:

- fail whole export;
- исключить optional Artifact с tombstone;
- пересобрать selection и запросить новое approval.

Scope не меняется незаметно.

### 31.4 Signing unavailable

Unsigned bundle не считается `vestrace-run-export` первой версии.

Временные bytes уничтожаются или помещаются в restricted failed-build area согласно policy. Они не выдаются как готовый export.

### 31.5 Secret scan failure

Запрещено пропускать scan failure как success. Поведение: exclusion, scope downgrade или fail closed.

### 31.6 Import tampering

Любое несовпадение checksum, root hash или signature:

- останавливает semantic parsing;
- помечает bundle `TamperedOrCorrupt`;
- сохраняет минимальный audit fact;
- не пытается «починить» archive;
- не доверяет частично прошедшим entries.

### 31.7 Missing trust anchor

Cryptographic signature может быть valid, но signer trust — unknown.

Bundle может оставаться в `IntegrityVerifiedTrustUnknown` для inspection, если policy разрешает. Migration/evaluation use блокируется до explicit trust decision.

## 32. Audit

H10 фиксирует минимум:

- export requested;
- normalized scope hash;
- authorization decision;
- approval reference;
- snapshot boundary;
- included/omitted object counts;
- classification summary;
- secret scan outcome;
- root hash;
- signer key revision/fingerprint;
- export Artifact revision;
- download/delivery decisions отдельно;
- import receipt;
- checksum/signature/trust outcome;
- workspace remap decision;
- selected import mode;
- final inert registration outcome.

Audit не содержит private keys, secret values или запрещённые payload bytes.

## 33. Public adapters

H11 может предоставить:

- CLI command для authorized export request/status/download;
- HTTP endpoints для create/status/download;
- SDK types;
- admin UI preview scope и omissions;
- inspect/import endpoints;
- verification report.

Все adapters используют один application contract.

Public API не предоставляет:

- arbitrary archive path selection;
- backend storage path;
- raw signing-key operation;
- «import and run» endpoint;
- «trust signer automatically» flag;
- bypass secret scan;
- unrestricted export-all-workspace command в рамках этого design.

MCP surface по умолчанию не предоставляет прямую выдачу полного bundle модели. Модель может предложить export request, но фактическое создание/доставка следует H2 policy и human/admin controls.

## 34. Verification report

Import/inspect создаёт bounded report:

```text
RunExportVerificationReport
├── format_supported
├── container_safe
├── checksums_valid
├── root_hash_valid
├── signature_valid
├── signer_trust
├── schemas_supported
├── workspace_binding
├── classification_result
├── secret_scan_result
├── entries_verified
├── entries_unsupported
├── tombstones
├── replayability
└── final_disposition
```

Допустимые dispositions:

```text
Rejected
Quarantined
IntegrityVerifiedTrustUnknown
InspectOnly
SupportCaseReady
EvaluationFixtureReady
MigrationStagingReady
```

Нет disposition `Activated` или `Running`.

## 35. Acceptance scenarios

### 35.1 Полный разрешённый terminal export

**Given:** terminal Run, все selected Artifacts разрешены, signer доступен.  
**When:** создаётся `PortableEvaluation` export.  
**Then:** journal, schemas, selected captures/Artifacts, audit proof, checksums и signature проходят independent verification.

### 35.2 Active Run snapshot

**Given:** Run продолжает выполняться.  
**When:** экспортируется exact version/sequence.  
**Then:** bundle помечен non-terminal и не содержит последующие events или Artifacts.

### 35.3 Purged capture

**Given:** context capture был hard-purged.  
**When:** создаётся export.  
**Then:** содержимое отсутствует, присутствует signed tombstone, replayability снижена без попытки реконструкции.

### 35.4 Classification denial

**Given:** selected Artifact имеет classification, запрещающую destination.  
**When:** запрашивается export.  
**Then:** операция denied либо требует нового narrower scope; Artifact не попадает в временный готовый bundle.

### 35.5 Tampered event file

**Given:** один byte `events.ndjson` изменён после signing.  
**When:** bundle импортируется.  
**Then:** checksum/root/signature verification fails, semantic parsing и registration не выполняются.

### 35.6 Unknown event schema

**Given:** signature valid, но target не поддерживает один schema version.  
**When:** bundle проверяется.  
**Then:** integrity может быть valid, semantic status — partial/unsupported, replay/resume заблокированы.

### 35.7 Cross-workspace без remap

**Given:** valid bundle из другого workspace.  
**When:** запрашивается `MigrationStaging` без approved remap.  
**Then:** import denied или остаётся в restricted inspection quarantine.

### 35.8 Inert inspection

**Given:** полностью valid bundle.  
**When:** выбран `InspectOnly`.  
**Then:** не создаются Run, work item, memory candidate, notification, credential lease или Tool invocation.

### 35.9 Valid signature, unknown signer

**Given:** cryptographic signature valid, trust anchor отсутствует.  
**When:** import policy разрешает limited inspection.  
**Then:** disposition `IntegrityVerifiedTrustUnknown`; migration/evaluation activation blocked.

### 35.10 Checkpoint included

**Given:** checkpoint разрешён policy.  
**When:** bundle импортируется.  
**Then:** checkpoint доступен только для inspection и не становится resumable H1 checkpoint.

### 35.11 Duplicate logical export request

**Given:** та же idempotency key и exact scope.  
**When:** request повторён.  
**Then:** не создаётся вторая логическая операция или неконтролируемый второй Artifact.

### 35.12 Secret discovered before signing

**Given:** final scan обнаруживает credential bytes.  
**When:** builder валидирует bundle.  
**Then:** entry исключается или export fails closed; unsigned unsafe bytes не выдаются.

## 36. Security invariants

1. Export не переносит domain authority.
2. Signature не является capability.
3. ID, hash, URI, path или signature не предоставляют доступ.
4. Bundle не содержит private signing key или usable credentials.
5. Approval связан с exact export operation hash.
6. Historical events не upcast-заменяются внутри signed evidence.
7. Unknown schema не интерпретируется как известная.
8. Purged bytes не реконструируются.
9. Import никогда не запускает side effects.
10. Workspace remap всегда explicit и authorized.
11. Source IDs не принимаются как local authority IDs.
12. Container metadata не влияет на semantic signature.
13. Лишняя или отсутствующая entry является verification error.
14. Redaction/scan failure не использует unsafe full-content fallback.
15. Valid signature без trust anchor не означает trusted source.
16. Audit-chain proof и export signature остаются разными доказательствами.
17. Imported approvals/policies являются historical evidence only.
18. Checkpoint import не разрешает resume.
19. Bundle-supplied executable upcasters запрещены.
20. Optional omissions всегда explicit и signed.

## 37. Documentation-only boundary

Этот design не разрешает:

- implementation branch;
- Rust modules;
- SQL migrations;
- archive parser/builder;
- signing backend;
- public endpoints;
- schema generation;
- real export/import;
- production keys;
- CI changes;
- автоматическую правку H1/H2/H5/H6/H8/H10/H11 plans.

После письменного утверждения отдельный normalization pass может добавить normative references в owning plans. Implementation plan создаётся отдельно.

## 38. Итоговое решение

> **`vestrace-run-export` — подписанный transport-neutral bundle точной Run snapshot boundary, event schemas, разрешённых Artifacts и audit evidence. Он проверяем, переносим и пригоден для инертной инспекции, но никогда не является permission, credential backup, automatic replay или Run activation package.**
