# Vestrace — workspace envelope encryption design

**Статус:** предложено для письменного утверждения  
**Дата:** 2026-08-01  
**Репозиторий:** `venm1r/vestrace`  
**Связанный boundary:** `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`  
**Capture profiles:** `docs/superpowers/specs/2026-07-31-vestrace-state-capture-profiles-adr.md`  
**Signed export:** `docs/superpowers/specs/2026-08-01-vestrace-signed-run-export-design.md`  
**Владельцы контрактов:** H6, H8 и H10; H2 разрешает операции, H11 предоставляет management/inspection surfaces

> Этот документ является documentation-only design. Он не разрешает менять Rust-код, создавать миграции, выпускать реальные ключи, шифровать существующие данные, выполнять rotation/erasure, изменять backup policy или запускать implementation branch.

## 1. Назначение

Документ определяет workspace-scoped envelope encryption для чувствительных payload Vestrace.

Цели:

- ограничить последствия компрометации storage backend;
- изолировать криптографические домены разных workspace;
- не хранить ключевой материал в PostgreSQL или H6 blob store;
- поддержать ротацию ключей без массового обязательного повторного шифрования bytes;
- поддержать смену алгоритма и DEK через resumable re-encryption;
- обеспечить криптографическое стирание чувствительных payload;
- сохранить append-only audit и content-free tombstones;
- не ломать RLS, optimistic concurrency, uniqueness, ordering и deterministic state transitions;
- сделать backup/restore поведение явным и проверяемым;
- не допустить plaintext fallback при недоступности key backend.

## 2. Нормативные принципы

> **Ключи шифрования не хранятся вместе с зашифрованными данными. PostgreSQL хранит только неприменимые сами по себе key references, wrapped DEK и криптографические metadata.**

> **Каждый workspace является отдельным криптографическим доменом. Ключ, ciphertext, wrapped DEK или content reference одного workspace не предоставляют право и возможность расшифровать данные другого workspace.**

> **Сбой шифрования или H8 key backend никогда не приводит к сохранению plaintext как fallback. Операция либо завершается fail-closed, либо optional capture понижается до безопасного режима без payload.**

> **Криптографическое стирание является необратимой административной операцией с точным scope, approval, audit и reconciliation. Оно не маскируется под обычное logical delete.**

## 3. Явно не входит

Этот design не является:

- full-disk encryption;
- заменой TLS;
- заменой PostgreSQL RLS;
- заменой H2 authorization;
- защитой от процесса, который уже законно получил plaintext и ключевую операцию;
- способом хранить API keys в State Engine;
- универсальным opaque encryption всех колонок;
- способом скрыть обязательные domain identifiers и state transitions;
- механизмом доверия к ciphertext без AEAD verification;
- cross-workspace deduplication;
- гарантией физического удаления всех копий ciphertext;
- автоматическим уничтожением backup media;
- механизмом экспорта ключей вместе с Run export;
- способом обойти H6 classification, quarantine, retention или purge;
- способом сделать скрытые рассуждения допустимыми для capture;
- механизмом восстановления уничтоженного ключевого материала;
- заменой H10 content-free security audit;
- новым secret backend или KMS implementation.

## 4. Владение контрактами

| Область | Владелец | Роль |
|---|---|---|
| Sensitive Artifact/context/capture bytes, retention, purge | H6 | определяет объект, classification, lifecycle и storage boundary |
| KEK references, wrapping/unwrapping, key generations, cryptographic leases | H8 | выполняет криптографические операции без выдачи raw KEK application layer |
| Authorization, risk, approval, administrative scope | H2 | разрешает create/rotate/rewrap/reencrypt/erase/restore operations |
| Security audit, capture facts, integrity outcomes | H10 | хранит content-free intents, results, failures и verification evidence |
| Public/CLI/admin surface | H11 | отображает безопасные статусы и запускает application commands |

State Engine не становится владельцем secret values. H6 не становится KMS. H8 не получает authority над Artifact lifecycle. H10 не хранит plaintext diagnostics о ключах.

## 5. Модель угроз

Design защищает прежде всего от следующих сценариев:

1. утечка H6 blob storage без доступа к H8 key backend;
2. утечка PostgreSQL backup без доступа к H8 key backend;
3. ошибочный доступ storage operator к raw blobs;
4. смешивание данных разных workspace;
5. восстановление старого backup после logical deletion;
6. частично выполненная rotation;
7. потеря одного DEK или одной key generation;
8. повторное использование nonce;
9. ciphertext substitution между объектами;
10. повреждение ciphertext или wrapped DEK;
11. plaintext fallback при outage;
12. утечка через cross-workspace equality/dedup;
13. сохранение derived indexes после erasure;
14. экспорт зашифрованных bytes без понятного key/import contract;
15. уничтожение ключа при неясном результате внешней H8 операции.

Design не обещает защиту, если attacker одновременно контролирует:

- приложение Vestrace с правом запрашивать decrypt;
- H8 backend и policy authority;
- memory процесса после успешной расшифровки.

Эти риски уменьшаются capability scope, short-lived leases, audit, process isolation и минимизацией plaintext lifetime.

## 6. Key hierarchy

Нормативная hierarchy:

```text
H8 root trust / secret backend
└── Workspace KEK generation reference
    └── wraps per-object/per-generation DEK
        └── encrypts one exact payload generation
```

### 6.1 Workspace KEK

Workspace Key Encryption Key:

- существует только в H8-compatible backend;
- application layer знает только opaque `WorkspaceKekGenerationRef`;
- scoped к exact workspace;
- имеет immutable generation identity;
- не экспортируется в Run export;
- не сохраняется в domain event payload;
- не появляется в Debug, logs, telemetry или errors;
- используется только для wrap/unwrap DEK и, при policy, derivation of scoped keyed fingerprints;
- lifecycle управляется отдельными H2-authorized commands.

### 6.2 DEK

Data Encryption Key:

- генерируется криптографически безопасно;
- имеет 256 bits entropy для начального algorithm profile;
- создаётся для одного encrypted object generation;
- не используется между workspace;
- не используется для двух независимых logical payload без явного deterministic encryption design, который в v0.2 запрещён;
- не хранится в plaintext после завершения cryptographic operation;
- хранится только wrapped под exact workspace KEK generation;
- уничтожение единственного usable wrapped DEK делает payload криптографически недоступным.

### 6.3 Object generation

Новый DEK обязателен при:

- создании нового sensitive object;
- создании новой immutable Artifact representation, если bytes отличаются;
- переходе на новый encryption algorithm profile;
- explicit DEK rotation;
- восстановлении объекта из plaintext source как новой generation;
- reclassification, если policy требует новый cryptographic boundary.

Обычная KEK rotation не требует нового DEK.

### 6.4 Key generation states

```text
Planned
Active
Retiring
Retired
DestroyPending
Destroyed
Compromised
Unavailable
OutcomeUnknown
```

Только `Active` generation используется для новых wrap operations.

`Retiring` и `Retired` могут использоваться для unwrap существующих DEK согласно policy, пока не завершён migration и не наступил destroy gate.

`Destroyed` необратим. Возврат в другое состояние запрещён.

## 7. Encryption algorithm profiles

Алгоритмы задаются repository-owned versioned registry.

Начальный обязательный профиль:

```text
AeadAes256GcmV1
```

Дополнительный профиль может быть утверждён отдельным ADR, например:

```text
AeadXChaCha20Poly1305V1
```

Один payload envelope фиксирует exact algorithm profile.

Algorithm profile задаёт:

- AEAD algorithm;
- key length;
- nonce length;
- tag length;
- nonce generation rules;
- associated-data canonicalization;
- wrapped-DEK format;
- key-backend operation contract;
- maximum plaintext size;
- streaming/chunking contract;
- error mapping;
- test vectors.

Запрещено:

- algorithm negotiation из untrusted payload;
- silent downgrade;
- ECB/CBC без отдельной integrity protection;
- повтор nonce под одним DEK;
- raw RSA encryption payload;
- пользовательский пароль как KEK;
- неаутентифицированное шифрование;
- самостоятельный выбор алгоритма моделью или extension.

## 8. Encrypted payload envelope

Концептуальная модель:

```text
EncryptedPayloadEnvelope
├── envelope_version
├── workspace_id
├── object_scope
│   ├── object_type
│   ├── object_id
│   ├── object_revision_or_generation
│   └── purpose
├── classification
├── algorithm_profile
├── kek_generation_ref
├── wrapped_dek
├── wrap_algorithm_profile
├── payload_nonce_or_stream_header
├── associated_data_schema_version
├── associated_data_hash
├── plaintext_length
├── ciphertext_length
├── ciphertext_hash
├── plaintext_content_hash_ref?
├── storage_locator
├── created_at
├── producer_revision
└── envelope_integrity_version
```

### 8.1 `wrapped_dek`

`wrapped_dek`:

- не является usable secret без H8 KEK;
- связан AEAD/Key-Wrap associated data с workspace, object identity, generation и algorithm profile;
- не переносится между workspace;
- заменяется при rewrap;
- имеет собственный deterministic hash для concurrency/integrity checks;
- никогда не принимается от модели, webhook или импортированного bundle как local authority.

### 8.2 Ciphertext hash

`ciphertext_hash`:

- проверяет exact stored bytes до decrypt;
- не заменяет AEAD tag verification;
- используется для storage corruption detection;
- может быть content-addressed physical locator input;
- не является plaintext equality proof.

### 8.3 Plaintext content hash

H6 может требовать canonical plaintext content hash для Artifact provenance.

Правила:

- hash остаётся classified metadata;
- он защищён RLS и export policy;
- physical encrypted storage path не строится напрямую из plaintext hash;
- cross-workspace dedup по plaintext hash запрещён;
- low-entropy equality risk должен учитываться policy;
- когда H6 semantics допускают keyed workspace fingerprint вместо global hash, используется versioned H8-backed keyed fingerprint;
- изменение существующего H6 hash contract требует отдельного H6 amendment, а не скрытой правки этим design.

## 9. Associated data

AEAD associated data обязательно связывает ciphertext с контекстом:

```text
workspace_id
object_type
object_id
object_generation
purpose
classification
algorithm_profile
kek_generation_id
producer_revision
associated_data_schema_version
```

Дополнительно могут входить:

- Artifact revision ID;
- capture record ID;
- event ID;
- context snapshot ID;
- retention class ID;
- plaintext hash, если H6 policy уже разрешает его хранение.

Associated data:

- canonicalized deterministically;
- не содержит secrets;
- проверяется перед выдачей plaintext;
- не меняется in-place;
- при изменении связанного immutable identity требует новой encrypted generation либо explicit rebind operation с повторным шифрованием.

Ciphertext substitution между workspace, object или purpose должен завершаться authentication failure.

## 10. Scope шифрования

### 10.1 Обязательно поддерживаемые классы

Envelope encryption применяется согласно classification/policy к:

- optional full/redacted context captures;
- sensitive H6 Artifact bytes;
- sensitive Artifact representations;
- model request/response captures, разрешённые capture policy;
- tool input/output captures;
- webhook quarantined bodies;
- imported bundle bytes в staging;
- sensitive evaluation fixtures;
- long-term memory content payload;
- bounded sensitive side payload в PostgreSQL;
- private support/forensic diagnostic captures;
- encrypted Run export package, когда destination policy требует confidentiality.

### 10.2 Что не шифруется opaque application envelope

Следующие поля не должны опаково шифроваться, если они нужны для authority и integrity:

- workspace ownership columns;
- RLS partition keys;
- immutable IDs;
- aggregate/run versions;
- state/status enums;
- sequence/cursor;
- optimistic-concurrency revision;
- operation fingerprint;
- idempotency key hash;
- lifecycle timestamps;
- bounded classification label;
- key-generation state;
- content-free audit intent/result;
- fields, необходимые для deterministic uniqueness constraint;
- fields, необходимые для безопасного recovery scheduling;
- ciphertext hash и envelope version.

Sensitive values внутри таких facts заменяются безопасными hashes/references, а не шифруются так, чтобы database constraints перестали работать.

### 10.3 Inline database payload

Для небольшого sensitive payload PostgreSQL хранит:

```text
EncryptedInlinePayload
├── envelope metadata
├── ciphertext bytes
└── no plaintext JSONB projection
```

Запрещено одновременно хранить plaintext и encrypted copy как silent cache.

Search/read model получает только разрешённые metadata либо отдельную производную projection, созданную under policy.

### 10.4 Large/blob payload

Large payload хранится в H6 blob store.

PostgreSQL содержит только:

- H6 object/revision identity;
- encrypted envelope metadata;
- wrapped DEK;
- ciphertext locator/hash;
- lifecycle/classification state;
- no raw plaintext.

## 11. Plaintext lifetime

Plaintext должен существовать минимально возможное время.

Правила:

- decrypt выполняется только после H2/H6 authorization;
- H8 unwrap lease имеет exact workspace, object purpose и expiry;
- plaintext не записывается в ordinary logs;
- plaintext не сохраняется во временный файл без отдельного encrypted temp contract;
- buffers очищаются best-effort через zeroizing types;
- plaintext не кэшируется между principals или requests;
- model/tool adapter получает только exact allowed slice;
- после cancellation результат decrypt не передаётся downstream;
- crash dump policy должна исключать sensitive process memory;
- error messages не включают plaintext fragments;
- panic/debug formatting для key/ciphertext envelopes редактируется.

## 12. H8 cryptographic ports

Концептуальные application ports:

```text
WorkspaceKeyPort
├── create_workspace_kek_generation
├── activate_workspace_kek_generation
├── wrap_dek
├── unwrap_dek
├── rewrap_dek
├── destroy_kek_generation
├── verify_key_generation_state
└── reconcile_unknown_key_operation
```

H8 backend возвращает:

- opaque key generation reference;
- operation receipt;
- provider/backend revision;
- bounded key status;
- no raw KEK;
- raw DEK только внутри tightly bounded cryptographic call, предпочтительно через in-process protected type.

Если backend поддерживает generate-data-key, plaintext DEK должен быть передан только encryption boundary и очищен после использования.

## 13. Authorization model

Разные operations требуют разных capabilities:

```text
encryption.read_sensitive
encryption.write_sensitive
encryption.rotate_kek
encryption.rotate_dek
encryption.change_algorithm
encryption.erase_object
encryption.erase_workspace_generation
encryption.inspect_status
encryption.restore_key_binding
encryption.manage_backup_policy
```

Role template не выдаёт эти полномочия автоматически. Реальная authority определяется scoped capability и H2 policy.

Capability scope включает:

- workspace;
- object type/IDs;
- operation;
- classification ceiling;
- key generation;
- time window;
- budget/maximum object count;
- risk ceiling;
- required approvals;
- destination/backup scope;
- reason code.

## 14. Risk levels и approvals

Рекомендуемый minimum risk:

| Operation | Minimum risk |
|---|---|
| Encrypt new sensitive object | medium |
| Decrypt for normal authorized use | medium |
| KEK generation activation | high |
| Bulk rewrap | high |
| Algorithm migration / DEK rotation | high |
| Object-level cryptographic erasure | high |
| Workspace generation destruction | critical |
| Restore historical key generation | critical |
| Change key backup/escrow policy | critical |

Context может повысить risk.

Critical operations требуют:

- exact object/generation inventory;
- independent verification;
- explicit user/admin approval;
- bounded execution window;
- checkpoint/backup assessment;
- dry-run report;
- post-operation reconciliation;
- immutable audit intent до внешней H8 operation.

## 15. Encryption write flow

Нормативный flow:

```text
plaintext produced or received
→ classification and payload policy
→ H2 authorization
→ select active workspace KEK generation
→ generate fresh DEK
→ construct canonical associated data
→ AEAD encrypt
→ compute ciphertext hash
→ H8 wrap DEK under exact KEK generation
→ atomically persist envelope metadata + H6 object reference
→ verify readback hash/metadata
→ zeroize plaintext DEK and plaintext buffers
→ append content-free audit result
```

Если persist после external H8 wrap завершился неоднозначно:

- operation получает `OutcomeUnknown`;
- reconciliation ищет exact operation receipt/idempotency key;
- новый DEK не генерируется blindly;
- orphan wrapped DEK удаляется только после подтверждённой reconciliation.

## 16. Decryption read flow

```text
read request
→ H2/H6 authorization
→ load envelope metadata
→ verify workspace/object binding
→ verify ciphertext hash and bounds
→ request H8 unwrap for exact KEK generation
→ AEAD decrypt with canonical associated data
→ verify authentication tag
→ return bounded plaintext to owning application service
→ zeroize buffers
→ record content-free access/audit fact when policy requires
```

Failure categories:

```text
Denied
KeyUnavailable
KeyDestroyed
EnvelopeCorrupt
CiphertextMissing
CiphertextHashMismatch
AuthenticationFailed
UnsupportedAlgorithm
UnsupportedEnvelopeVersion
ClassificationConflict
OutcomeUnknown
```

Ни одна ошибка не возвращает partial plaintext.

## 17. Rotation types

### 17.1 KEK rotation

KEK rotation меняет wrapping key generation, но сохраняет DEK и ciphertext.

```text
old KEK unwraps DEK
→ new KEK wraps same DEK
→ persist new wrapped-DEK generation
→ verify unwrap/decrypt under new generation
→ mark old wrapping retired for object
```

Преимущества:

- не читает и не переписывает large ciphertext;
- сохраняет plaintext/ciphertext hashes;
- минимизирует I/O;
- может выполняться resumably по inventory.

### 17.2 DEK rotation

DEK rotation создаёт новый DEK и новый ciphertext generation.

Требуется при:

- suspected DEK compromise;
- policy-mandated object rekey;
- migration away from old wrapping semantics, если simple rewrap недостаточен;
- object clone into a distinct cryptographic domain;
- reclassification requiring fresh encryption generation.

### 17.3 Algorithm migration

Algorithm migration всегда создаёт новую encrypted generation.

```text
read old envelope
→ authorized decrypt
→ generate new DEK/nonce
→ encrypt with new algorithm profile
→ persist new ciphertext/envelope
→ verify exact plaintext equivalence through protected comparison/hash policy
→ switch active generation
→ retire old generation
→ purge old ciphertext according to policy
```

Silent in-place change algorithm metadata запрещён.

### 17.4 Key compromise rotation

При compromise:

- generation немедленно перестаёт использоваться для новых writes;
- affected inventory фиксируется;
- read permission может быть приостановлена;
- rewrap допустим только если old KEK считается доступным и trustworthy для decrypt;
- если plaintext DEK мог быть раскрыт, требуется DEK rotation/re-encryption;
- risk/policy определяет urgency и isolation;
- audit не раскрывает key material.

## 18. Rotation job model

```text
KeyRotationJob
├── rotation_job_id
├── workspace_id
├── rotation_type
├── source_generation
├── target_generation
├── object_scope_manifest
├── policy_decision_id
├── approval_refs
├── expected_inventory_hash
├── status
├── cursor
├── succeeded_count
├── failed_count
├── unknown_count
├── created_at
├── started_at?
└── terminal_at?
```

Status:

```text
Planned
Approved
Running
Paused
VerificationRequired
Completed
CompletedWithExceptions
Failed
Cancelled
OutcomeUnknown
```

Rotation:

- restart-safe;
- idempotent per object generation;
- использует expected version;
- не переписывает completed history;
- фиксирует per-object result;
- не уничтожает source generation до exit gate;
- может быть приостановлена без потери уже подтверждённых результатов.

## 19. Rotation exit gate

Source KEK generation может перейти в `DestroyPending` только когда:

1. inventory snapshot зафиксирован;
2. все reachable encrypted objects обработаны либо имеют approved exception;
3. no object still references source generation как единственный usable wrapping;
4. decrypt verification для target generation прошла;
5. backup/restore impact оценён;
6. replication lag учтён;
7. active export/import operations завершены или отклонены;
8. H10 audit completeness verified;
9. independent verifier подтвердил report;
10. required approval всё ещё действителен.

## 20. Cryptographic erasure

### 20.1 Object-level erasure

Object-level erasure уничтожает возможность unwrap exact object DEK.

Нормативный flow:

```text
erasure request
→ exact object/generation inventory
→ retention/legal-hold check
→ H2 critical/high-risk authorization
→ immutable audit intent
→ revoke active reads
→ destroy wrapped DEK or dedicated key reference
→ reconcile H8 result
→ verify decrypt is impossible through supported paths
→ delete derived indexes/caches
→ schedule physical ciphertext purge
→ persist content-free erasure tombstone
```

Если один DEK используется более чем одним object generation, object-level erasure небезопасен. Такой sharing запрещён initial design.

### 20.2 Workspace-generation erasure

Destroying workspace KEK generation делает недоступными все DEK, которые остаются wrapped только этой generation.

Это critical bulk operation.

До destroy требуется:

- exact inventory;
- proof, что intended scope совпадает с affected objects;
- explicit approval;
- backup/escrow review;
- no pending rotation ambiguity;
- no legal hold conflict;
- dry-run impact report;
- independent verification.

### 20.3 Erasure tombstone

```text
CryptographicErasureTombstone
├── erasure_id
├── workspace_id
├── scope_type
├── object_refs_or_inventory_hash
├── key_generation_refs
├── reason_code
├── policy_decision_id
├── approval_refs
├── requested_by
├── requested_at
├── completed_at
├── verification_result
├── derived_projection_cleanup_state
├── physical_purge_state
└── content_recovery = Impossible | Unknown
```

Tombstone не содержит key bytes, plaintext, secret backend path или sensitive content summary.

### 20.4 Difference from physical deletion

Cryptographic erasure:

- уничтожает доступность plaintext;
- может завершиться до удаления всех ciphertext copies;
- требует key-destruction proof/reconciliation.

Physical deletion:

- удаляет ciphertext bytes из live storage, caches, replicas и backups согласно lifecycle;
- может быть delayed;
- не заменяет key destruction, если backup still exists.

Обе операции отслеживаются отдельно.

## 21. Unknown outcome при key destruction

Если H8 backend timeout произошёл после destroy request:

- key generation получает `OutcomeUnknown`;
- нельзя повторять destroy blindly, если backend operation semantics не idempotent;
- нельзя считать key доступным или уничтоженным;
- reads и new writes через generation приостанавливаются;
- reconciliation запрашивает provider receipt/status;
- user/admin получает high-priority intervention, если status нельзя определить;
- erasure tombstone остаётся incomplete.

## 22. Backup policy

### 22.1 Separation

Backup разделяется на:

```text
Data backup
Key-backend backup / escrow policy
```

Data backup без key material остаётся зашифрованным.

Key backup не должен находиться в том же trust domain и access path, что data backup, без явного critical-risk решения.

### 22.2 Restore requirements

Restore обязан определить:

- source workspace cryptographic identity;
- required KEK generations;
- availability и trust status каждой generation;
- envelope/algorithm versions;
- key-provider/backend revision;
- deleted/destroyed generations;
- legal retention state;
- whether restore is same-workspace or remapped workspace.

Restore не может:

- silently create replacement key for old ciphertext;
- map source workspace key to target workspace automatically;
- treat missing key as empty payload;
- remove erasure tombstones;
- resurrect cryptographically erased content from data backup alone.

### 22.3 Restore without key material

Если key material недоступен:

- metadata и content-free audit могут быть восстановлены;
- encrypted object state становится `KeyUnavailable` или `KeyDestroyed`;
- payload остаётся unreadable;
- search/index representations удаляются или не восстанавливаются;
- system не пытается reconstruct content;
- operator получает exact impact report.

### 22.4 Backup and erasure

Cryptographic erasure считается завершённым только если policy определяет судьбу всех key replicas/escrow copies.

Возможные outcomes:

```text
ErasedFromAllAuthorizedKeyCopies
ErasedFromPrimaryPendingBackupExpiry
BlockedByLegalHold
BlockedByEscrowPolicy
OutcomeUnknown
```

Нельзя утверждать complete erasure, если usable key copy остаётся в доступном escrow.

## 23. Cross-workspace behavior

Cross-workspace decrypt запрещён.

Копирование sensitive object в другой workspace выполняется как отдельная authorized operation:

```text
source decrypt under source policy
→ classification/export/share decision
→ plaintext bounded transfer inside application boundary
→ fresh target DEK
→ encrypt under target workspace KEK
→ new target object/revision with provenance
```

Запрещено:

- переносить wrapped DEK как target authority;
- использовать source KEK в target workspace;
- сохранять один ciphertext object с двумя workspace owners;
- deduplicate encrypted bytes across workspace;
- делать source plaintext hash публичным equality oracle;
- считать mount/share permission правом на raw decrypt.

Cross-workspace memory sharing получает отдельный design и использует этот re-encryption boundary только после explicit grant.

## 24. Deduplication

### 24.1 Default

Для encrypted sensitive objects deduplication отключена по умолчанию, особенно cross-workspace.

Причины:

- equality leakage;
- complex erasure semantics;
- shared-DEK risk;
- shared-ciphertext lifecycle conflicts;
- classification mismatch;
- source/target provenance ambiguity.

### 24.2 Future same-workspace dedup

Same-workspace dedup может быть рассмотрена отдельным ADR только если:

- каждый logical object сохраняет независимую key revocation semantics;
- shared bytes не требуют shared DEK;
- deletion одного object не нарушает другой;
- equality leakage принята policy;
- accounting/provenance remain exact;
- cryptographic erasure scope остаётся доказуемым.

Initial implementation не должна зависеть от encrypted dedup.

## 25. Search, embeddings и derived projections

Encrypted plaintext не становится автоматически searchable.

Indexing flow:

```text
authorized decrypt
→ policy permits indexing exact fields
→ create minimized derived representation
→ classify and bind to source generation
→ store projection with source encryption/erasure references
```

Derived data включает:

- FTS lexemes;
- embeddings;
- excerpts;
- thumbnails;
- OCR/text extraction;
- summaries;
- caches;
- evaluation fixtures.

Правила:

- projection содержит source object/generation ID;
- projection наследует classification ceiling;
- erasure source payload запускает cleanup projection;
- stale projection cannot survive cryptographic erasure unnoticed;
- embedding vector считается sensitive derived data;
- model-generated summary не считается безопасной автоматически;
- search result не может раскрыть data principal без authorization;
- missing source key делает projection invalid for serving.

## 26. Capture profiles

State capture profile задаёт только baseline preference.

- `Minimal` обычно не создаёт sensitive optional payload.
- `Operational` сохраняет bounded structured metadata.
- `Reproducible` может создавать redacted encrypted H6 Artifacts.
- `Forensic` может создавать full encrypted capture только при explicit policy/expiry.

Encryption не расширяет capture permission.

Факт, что payload будет зашифрован, не разрешает сохранять:

- hidden chain-of-thought;
- provider reasoning tokens;
- secrets;
- unrestricted personal data;
- content, запрещённый policy.

## 27. Run export interaction

Signed Run export:

- не включает workspace KEK;
- не включает raw/wrapped local DEK как usable import key authority;
- может включать ciphertext only при explicit encrypted-package profile;
- должен различать source-at-rest encryption и export-package encryption;
- package encryption использует отдельный recipient/package key contract;
- import не привязывает source key references к target H8 backend автоматически;
- plaintext export требует отдельного H2/H6 authorization;
- erased payload представлен tombstone.

At-rest envelope metadata может быть включена только как provenance, не как обещание возможности decrypt на target deployment.

## 28. Event and audit behavior

Domain events сохраняют content-free facts:

```text
WorkspaceKekGenerationCreated
WorkspaceKekGenerationActivated
EncryptionEnvelopeCreated
DekWrapped
DekRewrapped
KeyRotationStarted
KeyRotationPaused
KeyRotationCompleted
KeyRotationCompletedWithExceptions
KeyGenerationDestroyRequested
KeyGenerationDestroyed
KeyGenerationOutcomeUnknown
ObjectCryptographicErasureRequested
ObjectCryptographicallyErased
EncryptedPayloadIntegrityFailed
EncryptedPayloadKeyUnavailable
DerivedProjectionErasureStarted
DerivedProjectionErasureCompleted
```

Events не содержат:

- raw KEK/DEK;
- wrapped DEK bytes, если audit не требует exact hash/reference;
- plaintext;
- secret backend path;
- provider authentication material;
- unrestricted error body.

H10 security audit intent фиксируется до:

- activation generation;
- bulk rotation;
- destroy request;
- restore binding;
- change escrow policy;
- cross-workspace re-encryption.

## 29. Retention

Retention применяется независимо к:

- ciphertext;
- envelope metadata;
- wrapped DEK;
- content hash/fingerprint;
- derived projections;
- audit/tombstone;
- backup copies.

Destroying DEK before retention expiry допускается только explicit erasure policy.

Deleting ciphertext without deleting wrapped DEK создаёт `CiphertextMissing`, но не считается cryptographic erasure.

Deleting wrapped DEK without tombstone и audit запрещено.

Content-free tombstones могут храниться дольше, чем payload, чтобы объяснить отсутствие данных и предотвратить ложное восстановление.

## 30. Failure semantics

### 30.1 H8 unavailable on write

- sensitive mandatory write fails closed;
- optional capture becomes `Disabled`/`CaptureUnavailable`;
- plaintext не сохраняется временно;
- retry допускается только как new attempt той же idempotent command;
- operation не помечается succeeded.

### 30.2 H8 unavailable on read

- return `KeyUnavailable`;
- no plaintext;
- Run может pause/fail/wait согласно owning policy;
- ordinary metadata reads могут продолжаться;
- no automatic fallback to another workspace key.

### 30.3 Ciphertext corrupt

- object quarantined;
- no partial plaintext;
- integrity failure audited;
- replica repair допустим только по verified ciphertext hash;
- key rotation не используется как способ скрыть corruption.

### 30.4 Wrapped DEK corrupt

- unwrap fails closed;
- object state `EnvelopeCorrupt`;
- restore from verified metadata backup may be attempted;
- new DEK не может decrypt old ciphertext;
- content не reconstruct’ится моделью.

### 30.5 Nonce collision detected

- encryption result rejected before commit;
- DEK/nonce regenerated;
- security audit emitted;
- repeated collision triggers backend/entropy health suspension.

### 30.6 Rotation interruption

- completed object generations remain valid;
- source generation not destroyed;
- cursor resumes from durable checkpoint;
- no object is considered migrated without decrypt verification under target wrapping;
- failures remain explicit exceptions.

### 30.7 Erasure cleanup partial failure

- cryptographic erasure status and physical/projection cleanup status remain separate;
- source key destruction is not rolled back;
- cleanup retries are safe/idempotent;
- public UI shows incomplete derived/physical cleanup without claiming content recoverability.

## 31. Concurrency

Encrypted object metadata uses optimistic versioning.

Commands include:

- expected object revision;
- expected envelope generation;
- expected active KEK generation;
- idempotency key;
- exact operation fingerprint.

Concurrent operations:

- rotate vs erase: erase wins only through explicit policy arbitration; both cannot commit silently;
- read vs destroy: new decrypt leases denied after destroy intent reaches protected cutoff;
- rewrap vs re-encrypt: owning rotation job serializes object generation transition;
- purge vs export: exact snapshot/lease determines permitted result;
- restore vs erasure tombstone: tombstone prevents activation of erased payload.

## 32. Key access leases

H8 key operation lease:

```text
KeyOperationLease
├── lease_id
├── workspace_id
├── operation
├── object_scope
├── key_generation_ref
├── policy_decision_id
├── principal_or_service_identity
├── valid_from
├── valid_until
├── maximum_operations
└── nonce
```

Lease:

- short-lived;
- non-transferable;
- not stored in Run context;
- not logged;
- does not expose key bytes;
- revoked when operation scope changes;
- insufficient without H6 object authorization.

## 33. Public and administrative surfaces

H11 may expose:

- encryption enabled state;
- active/retiring key generation IDs as opaque safe identifiers;
- algorithm profile;
- rotation progress;
- affected object counts;
- key availability state;
- erasure/purge status;
- backup compatibility status;
- integrity failures;
- last verified operation timestamps.

H11 must not expose:

- raw wrapped DEK;
- secret backend path;
- KMS credentials;
- raw provider errors;
- plaintext hashes without policy;
- object-level sensitive inventory to unauthorized principals;
- capability tokens;
- exact internal nonce when unnecessary.

Administrative commands require typed confirmation, exact scope preview and approval binding.

## 34. Metrics

Allowed bounded metrics:

```text
encryption_operations_total{operation,outcome,algorithm}
encryption_operation_duration_seconds{operation,algorithm}
key_rotation_objects_total{outcome}
key_rotation_jobs{state}
key_backend_requests_total{operation,outcome}
encrypted_payload_integrity_failures_total{category}
cryptographic_erasure_operations_total{scope,outcome}
derived_cleanup_backlog{projection_type}
```

Metric labels не содержат:

- workspace names;
- object IDs;
- key refs;
- paths;
- plaintext/content hashes;
- secret backend responses.

## 35. Verification

### 35.1 Envelope verification

Проверяет:

- supported envelope version;
- exact workspace/object binding;
- algorithm profile;
- ciphertext hash;
- wrapped DEK metadata;
- associated-data hash;
- key generation state;
- AEAD authentication;
- plaintext length bounds.

### 35.2 Rotation verification

Проверяет:

- every inventory item classified;
- target wrapped DEK usable;
- decrypt under target equals original protected plaintext/hash contract;
- no unintended source references remain;
- exceptions explicit;
- source destroy gate not crossed early.

### 35.3 Erasure verification

Проверяет:

- H8 destroy receipt/state;
- unwrap impossible through supported path;
- active leases revoked/expired;
- derived projections scheduled/removed;
- restore policy cannot silently resurrect key;
- tombstone complete;
- backups/escrow accurately reflected.

Verification не пытается decrypt erased content как routine check after confirmed destruction. Она подтверждает key-state and path unavailability through H8 evidence and controlled negative tests.

## 36. Acceptance scenarios

### Scenario 1: New sensitive Artifact

Given:

- active workspace KEK generation;
- H2 permit;
- sensitive H6 Artifact bytes.

Then:

- fresh DEK generated;
- bytes encrypted with AEAD;
- DEK wrapped by H8;
- ciphertext stored;
- plaintext absent from PostgreSQL/blob store;
- readback verification succeeds;
- audit contains no key/plaintext.

### Scenario 2: H8 unavailable during optional capture

Then:

- capture disabled or marked unavailable;
- production Run continues only if capture non-mandatory;
- plaintext not written;
- replayability reduced explicitly.

### Scenario 3: H8 unavailable during mandatory sensitive write

Then:

- operation fails/pauses closed;
- no plaintext fallback;
- no success event;
- bounded error fact recorded.

### Scenario 4: KEK rotation

Then:

- new KEK generation active for new writes;
- existing DEK rewrapped resumably;
- ciphertext bytes unchanged;
- source generation not destroyed before exit gate.

### Scenario 5: Algorithm migration

Then:

- new DEK and ciphertext generation created;
- original event/Artifact lineage preserved;
- old generation retired only after verification;
- metadata never claims new algorithm for old ciphertext.

### Scenario 6: Object cryptographic erasure

Then:

- exact DEK path destroyed;
- decrypt no longer possible;
- tombstone persisted;
- derived FTS/vector/excerpt cleanup triggered;
- physical ciphertext purge tracked separately.

### Scenario 7: Destroy request timeout

Then:

- state becomes `OutcomeUnknown`;
- reads paused;
- no blind retry;
- reconciliation checks H8 receipt/status;
- completion not claimed.

### Scenario 8: Restore backup without old KEK

Then:

- metadata restored;
- object state key-unavailable/destroyed;
- no replacement key fabricated;
- payload not served or indexed.

### Scenario 9: Cross-workspace copy

Then:

- source authorization checked;
- target authorization checked;
- plaintext bounded within application boundary;
- fresh target DEK used;
- source wrapped DEK not reused;
- target provenance references source safely.

### Scenario 10: Ciphertext substitution

Given ciphertext from another object/workspace.

Then:

- associated-data/AEAD verification fails;
- no plaintext returned;
- integrity incident recorded.

### Scenario 11: Erased object appears in old data backup

Then:

- missing/destroyed key keeps bytes unreadable;
- tombstone prevents activation;
- restore report explains non-recoverability.

### Scenario 12: Forensic capture expiry

Then:

- new full captures stop;
- existing encrypted captures follow retention;
- expiry does not auto-decrypt or export bytes;
- scheduled erasure/purge occurs under policy.

## 37. Security invariants

1. Raw KEK never enters PostgreSQL, H6 blob store, Run context, logs or exports.
2. DEK is fresh per encrypted object generation.
3. Nonce is never reused under one DEK.
4. AEAD associated data binds workspace, object, generation and purpose.
5. Cross-workspace decrypt and dedup are forbidden.
6. Encryption failure never falls back to plaintext.
7. Key references, hashes and ciphertext are not capabilities.
8. Opaque encryption does not remove RLS/authorization requirements.
9. Destroying key generation requires exact inventory and critical approval.
10. Unknown H8 outcome never becomes assumed success.
11. Erasure tombstone remains after payload becomes unreadable.
12. Derived indexes are part of erasure scope.
13. Backup key copies are included in erasure truthfulness.
14. Hidden reasoning and secrets remain forbidden even when encryption is available.
15. Algorithm/profile changes are versioned and never silent.
16. Historical hashes/events are not rewritten by rotation.
17. Import/export never transfers local KEK authority automatically.
18. Same key material is never shared between workspace.
19. Plaintext lifetime and surface are minimized.
20. No model or extension controls key generation, algorithm registry or destroy scope.

## 38. Data model guidance

Future implementation may introduce entities such as:

```text
WorkspaceKeyPolicy
WorkspaceKekGeneration
EncryptedPayloadEnvelope
EncryptedObjectGeneration
KeyRotationJob
KeyRotationItem
CryptographicErasureRequest
CryptographicErasureTombstone
DerivedProjectionCleanupJob
KeyOperationReceipt
```

Это guidance, а не разрешение на миграции.

Persisted types должны быть Vestrace-owned. Provider SDK/KMS types остаются в H8 adapters.

## 39. Suggested implementation boundaries

Future implementation should separate:

```text
vestrace-domain
  encryption metadata, lifecycle, invariants

vestrace-application
  authorization-aware encryption/rotation/erasure services

vestrace-credential-runtime / H8 adapter
  KEK creation, wrap/unwrap, destroy, reconciliation

vestrace-artifact runtime / H6
  ciphertext storage, retention, physical purge

vestrace-observability / H10
  audit intents, outcomes, verification

vestrace-channel / H11
  typed admin commands and safe read models
```

No adapter may bypass owning application service and mutate encryption lifecycle directly.

## 40. Rollout order

1. Approve this design.
2. Normalize H6/H8/H10/H11 plan references.
3. Define exact algorithm registry and test vectors.
4. Define key backend capability matrix.
5. Define migration strategy for existing plaintext payload.
6. Implement read support for encrypted envelopes.
7. Implement write path disabled by default.
8. Verify backup/restore in staging.
9. Enable encryption for new optional captures in one test workspace.
10. Implement resumable KEK rotation.
11. Implement DEK/algorithm re-encryption.
12. Implement erasure and derived cleanup only after independent verification.
13. Migrate existing sensitive payload in bounded batches.
14. Enable per workspace under explicit policy.

Readers and recovery tooling must support encrypted envelopes before producers begin writing them.

## 41. Migration principles

Existing plaintext data is not silently considered encrypted.

Migration requires:

- exact inventory;
- classification;
- object-by-object state;
- temporary protected read;
- encryption and readback verification;
- atomic pointer/generation switch;
- plaintext removal verification;
- cache/index review;
- backup implications;
- rollback before plaintext destruction;
- no rollback after cryptographic erasure.

Migration failures remain explicit and do not mark workspace fully encrypted.

## 42. Open implementation choices intentionally deferred

После утверждения design отдельный implementation plan должен выбрать:

- concrete H8 backend(s) for local deployment;
- exact AES-GCM nonce generation implementation;
- streaming/chunked encryption format for large blobs;
- secure memory/zeroization crates;
- PostgreSQL envelope representation;
- key-operation receipt schema;
- migration numbering;
- backup escrow support or explicit absence;
- performance limits and batching;
- operational recovery procedures.

Эти choices не меняют нормативные границы документа.

## 43. Review gate

Слияние этого design утверждает только:

- workspace-scoped KEK hierarchy;
- per-object DEK envelope encryption;
- separation of KEK rewrap and DEK/algorithm re-encryption;
- fail-closed behavior;
- cryptographic erasure semantics;
- backup/restore and derived-projection obligations;
- cross-workspace isolation;
- ownership H6/H8/H10/H2/H11.

Слияние не разрешает:

- создавать ключи;
- менять H8 backend;
- шифровать production data;
- создавать migrations;
- включать encryption;
- выполнять rotation/erasure;
- менять backup/escrow policy;
- начинать implementation branch.
