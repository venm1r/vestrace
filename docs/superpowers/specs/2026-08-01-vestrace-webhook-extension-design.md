# Vestrace — webhook extension boundary design

**Статус:** предложено для письменного утверждения  
**Дата:** 2026-08-01  
**Репозиторий:** `venm1r/vestrace`  
**Связанный boundary:** `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`  
**Совместимость событий:** `docs/superpowers/specs/2026-07-31-vestrace-event-schema-compatibility-adr.md`  
**Владельцы контрактов:** H2, H7, H8, H10 и H11

> Этот документ является documentation-only design. Он не разрешает менять Rust-код, создавать миграции, выпускать webhook secrets, открывать сетевые endpoints, выполнять реальные доставки или активировать webhook definitions.

## 1. Назначение

Документ определяет безопасную extension boundary для входящих и исходящих webhooks Vestrace.

Webhook support предназначен для:

- приёма проверенных внешних событий;
- создания H7 observations и trigger occurrences;
- уведомления внешних систем о разрешённых Vestrace events;
- интеграции self-hosted deployments без прямого доступа к PostgreSQL;
- типизированной, версионируемой и аудируемой доставки;
- restart-safe retry и reconciliation;
- изоляции внешних payload от авторитетного Run state.

Webhooks являются adapters над существующими application services. Они не создают новый event store, новый trigger runtime, новый policy engine или отдельную authority boundary.

## 2. Нормативные принципы

> **Входящий webhook создаёт только проверенный и ограниченный source fact. Он никогда напрямую не изменяет Run, approval, budget, memory, Artifact или policy state.**

> **Исходящий webhook доставляет версионированное представление уже зафиксированного source event через durable delivery intent. Успех или сбой доставки не переписывает source state.**

> **Webhook endpoint, URL, secret reference, event ID, signature или delivery receipt не являются capability. Любое действие проходит H2 authorization и owning application port.**

## 3. Явно не входит

Webhook boundary не является:

- произвольным HTTP proxy;
- механизмом прямого вызова внутренних SQL operations;
- способом удалённого изменения Run status;
- универсальным RPC поверх JSON;
- заменой H7 public event stream;
- заменой H7 typed HumanResponse;
- способом выдачи approval из произвольного текста;
- способом передачи password, token, private key или refresh token;
- unrestricted callback URL, выбранным моделью;
- механизмом автоматической установки capabilities;
- механизмом обхода trigger autonomy ceilings;
- гарантией exactly-once доставки через сеть;
- способом доверять payload только потому, что TLS соединение успешно;
- способом интерпретировать внешний текст как system instructions;
- transport для hidden chain-of-thought;
- способом регистрировать canonical Vestrace event kinds извне;
- обязательным включённым по умолчанию feature.

## 4. Владение контрактами

| Область | Владелец | Роль webhook boundary |
|---|---|---|
| Authorization, risk, approvals, capabilities | H2 | разрешает endpoint management, source evaluation, delivery и exact consequences |
| Triggers, interactions, source facts, public events | H7 | владеет inbound occurrence, trigger evaluation и outbound source events |
| Connections, secret references, signing leases | H8 | хранит endpoint credentials вне domain payload и выдаёт bounded cryptographic operations |
| Audit, telemetry, diagnostics | H10 | фиксирует content-free security facts и bounded delivery metrics |
| HTTP/API/SDK/UI adapters | H11 | предоставляет management и inspection surfaces над общими application services |

H1 `AgentRun` остаётся единственным владельцем execution state. H6 владеет крупными payload/attachments и quarantine Artifact lifecycle, когда inline payload превышает разрешённую форму.

## 5. Feature state и default posture

Webhook support отключён по умолчанию на двух уровнях:

```text
Deployment webhook capability = Disabled
Workspace webhook capability  = Disabled
```

Endpoint может быть активирован только если одновременно:

1. deployment policy разрешает webhook direction;
2. workspace policy разрешает endpoint class;
3. principal имеет требуемую capability;
4. H2 policy permit действует для exact endpoint revision;
5. H8 secret/key references существуют и доступны;
6. endpoint прошёл validation;
7. lifecycle state равен `Active`.

Наличие сохранённого endpoint definition не означает, что endpoint активен.

## 6. Основные сущности

### 6.1 WebhookEndpoint

```text
WebhookEndpoint
├── endpoint_id
├── workspace_id
├── direction
├── display_name
├── lifecycle_state
├── active_revision_id?
├── created_by
├── state_revision
├── created_at
└── updated_at
```

Direction:

```text
Inbound
Outbound
```

Lifecycle state:

```text
Draft
Disabled
Active
Suspended
Degraded
Revoked
Deleted
```

`WebhookEndpoint` содержит mutable lifecycle identity. Поведение endpoint определяется только immutable revision.

### 6.2 WebhookEndpointRevision

```text
WebhookEndpointRevision
├── endpoint_revision_id
├── endpoint_id
├── revision_number
├── direction
├── protocol_version
├── source_or_destination
├── event_contracts
├── authentication_profile
├── signing_profile
├── allowlist_policy
├── network_policy
├── payload_policy
├── replay_policy
├── retry_policy
├── retention_policy
├── classification_ceiling
├── trigger_or_subscription_binding
├── created_by
├── policy_decision_id
├── canonical_hash
└── created_at
```

Revision неизменяема. Любая правка URL, allowlist, signing algorithm, event mapping, maximum bytes, retry policy или trigger binding создаёт новую revision.

### 6.3 InboundWebhookReceipt

```text
InboundWebhookReceipt
├── receipt_id
├── workspace_id
├── endpoint_revision_id
├── request_fingerprint
├── external_event_id_hash?
├── signature_result
├── timestamp_result
├── replay_result
├── allowlist_result
├── payload_state
├── body_hash
├── source_fact_id?
├── trigger_occurrence_id?
├── disposition
├── recorded_at
└── retention_class
```

Disposition:

```text
Accepted
AcceptedDuplicate
RejectedAuthentication
RejectedReplay
RejectedAllowlist
RejectedPayload
RejectedSchema
RejectedPolicy
Quarantined
SuspendedEndpoint
```

Receipt является content-bounded security record. Raw secrets и unrestricted headers в нём не сохраняются.

### 6.4 WebhookSourceFact

```text
WebhookSourceFact
├── source_fact_id
├── workspace_id
├── endpoint_revision_id
├── external_event_type
├── external_event_id_hash?
├── occurred_at?
├── recorded_at
├── trust_classification
├── normalized_payload
├── payload_artifact_revision_id?
├── payload_hash
├── schema_revision
├── dedup_key
└── quarantine_state
```

`WebhookSourceFact` — проверенная запись о полученном внешнем сообщении. Он всё равно считается external/untrusted data и не становится instruction или authority.

### 6.5 OutboundWebhookSubscription

```text
OutboundWebhookSubscription
├── subscription_id
├── workspace_id
├── endpoint_revision_id
├── source_event_filter_revision
├── payload_template_revision
├── classification_policy_revision
├── state
├── state_revision
├── created_by
└── created_at
```

Filter и template используют bounded declarative contract. Arbitrary code, SQL, shell, unbounded regex и model-generated executable templates запрещены.

### 6.6 WebhookDeliveryIntent

```text
WebhookDeliveryIntent
├── delivery_id
├── workspace_id
├── subscription_id
├── endpoint_revision_id
├── source_event_ref
├── source_event_hash
├── payload_schema_version
├── payload_hash
├── idempotency_key
├── attempt_policy
├── status
├── available_at
├── created_at
└── terminal_at?
```

Status:

```text
Pending
Delivering
Succeeded
FailedRetryable
FailedPermanent
OutcomeUnknown
Cancelled
DeadLetter
```

### 6.7 WebhookDeliveryAttempt

```text
WebhookDeliveryAttempt
├── attempt_id
├── delivery_id
├── attempt_number
├── request_fingerprint
├── signing_key_revision_ref
├── started_at
├── completed_at?
├── transport_result
├── http_status_class?
├── response_body_hash?
├── receiver_receipt_hash?
├── completion_may_have_occurred
└── bounded_error_category?
```

Одна logical delivery может иметь несколько attempts. Повтор не создаёт второй source event.

## 7. Inbound processing pipeline

Нормативный pipeline:

```text
HTTP request
→ endpoint revision lookup
→ endpoint lifecycle check
→ connection and proxy normalization
→ origin / network allowlist check
→ bounded header validation
→ body size and content-type gate
→ signature verification
→ timestamp and replay-window verification
→ external event ID / nonce deduplication
→ schema validation
→ quarantine and classification
→ durable receipt + source fact commit
→ H7 trigger evaluation
→ optional observation / proposal / bounded Run creation under H2
```

Ни один этап до durable source fact не может вызывать Run mutation.

### 7.1 Endpoint routing

Inbound route использует opaque endpoint handle. Handle:

- имеет достаточную энтропию;
- не содержит workspace ID или secret;
- не заменяет signature/authentication;
- может быть отозван заменой revision или endpoint lifecycle;
- не отображается модели как credential.

Unknown handle возвращает одинаковый bounded response без раскрытия существования workspace или endpoint.

### 7.2 Network и origin checks

Allowlist может включать:

- точные CIDR ranges;
- mTLS identity references;
- verified provider identity;
- exact host/origin metadata, если она криптографически связана с запросом;
- deployment-approved reverse-proxy identity.

`X-Forwarded-For`, `Forwarded` и аналогичные headers доверяются только от заранее разрешённого proxy chain.

DNS name сам по себе не является стабильной origin identity.

### 7.3 Headers

Разрешён только bounded allowlist headers, необходимых protocol profile:

- signature;
- signature version/key ID;
- request timestamp;
- event ID/nonce;
- event type;
- content type;
- optional trace link.

Запрещено сохранять wholesale request headers. Authorization headers, cookies и proxy credentials никогда не попадают в ordinary event payload или telemetry.

### 7.4 Payload bounds

Endpoint revision задаёт:

```text
maximum_wire_bytes
maximum_decoded_bytes
maximum_json_depth
maximum_array_items
maximum_string_bytes
allowed_content_types
allowed_compression
maximum_expansion_ratio
schema_revision
```

Compression bombs, archive payloads, multipart files и executable content запрещены по умолчанию.

Large allowed payload сохраняется через H6 quarantine Artifact flow. Inline source fact содержит только bounded normalized fields, hash и Artifact reference.

### 7.5 Signature verification

Supported profiles определяются versioned registry. Начальные допустимые классы:

```text
HMAC-SHA-256
Ed25519
ProviderSpecificVerifiedProfile
mTLSBoundProfile
```

Точное wire format каждой подписи является отдельным versioned contract.

Правила:

- подпись проверяется по exact raw body bytes или явно нормализованному provider contract;
- canonicalization не угадывается;
- algorithm downgrade запрещён;
- key ID выбирает разрешённую H8 key revision, но не произвольный key;
- secret bytes не покидают H8 cryptographic boundary;
- constant-time comparison обязателен для MAC;
- invalid signature не передаётся в trigger evaluation;
- отсутствие подписи допустимо только для явно утверждённого mTLS/private-network profile, а не как silent fallback.

### 7.6 Timestamp и replay protection

Endpoint revision задаёт:

```text
maximum_clock_skew
replay_window
required_nonce_or_event_id
nonce_retention
```

Проверка использует trusted server clock. External timestamp не становится `recorded_at`.

Request отклоняется, если:

- timestamp отсутствует при обязательном profile;
- timestamp за пределами окна;
- nonce/event ID уже использован;
- signature не связывает timestamp и event ID, когда protocol требует это;
- key revision отозвана до допустимого validation point.

### 7.7 Deduplication

Dedup key строится из versioned contract, например:

```text
H(endpoint_revision_id || external_event_id || body_hash)
```

Повтор валидного идентичного события:

- не создаёт второй source fact;
- не создаёт второй trigger occurrence;
- возвращает bounded success/duplicate response;
- фиксирует duplicate receipt без повторного исполнения.

Одинаковый external event ID с другим body hash переводится в conflict/quarantine и не исполняется.

### 7.8 Schema validation

Inbound payload не определяет собственную schema version произвольно. Endpoint revision связывает разрешённые external event kinds с exact schemas.

Unknown kind/version:

- может быть сохранён в quarantine при policy;
- не становится canonical Vestrace event;
- не запускает trigger;
- не upcast’ится эвристически;
- получает bounded compatibility outcome.

### 7.9 Quarantine и trust

Даже после успешной signature verification payload остаётся внешними данными.

Trust classes:

```text
VerifiedTransportUntrustedContent
VerifiedProviderEvent
AuthenticatedPrivateSource
UnverifiedExternal
Quarantined
```

Signature доказывает владение ключом/идентичность transport source, но не истинность payload и не право расширить permissions.

### 7.10 Atomic persistence

Для принятого inbound request в одной PostgreSQL transaction фиксируются:

- dedup claim;
- receipt;
- normalized source fact;
- H10 audit intent;
- optional H7 trigger-evaluation work item/outbox intent.

Trigger evaluation выполняется после durable commit. Сбой worker не теряет source fact.

## 8. H7 trigger boundary

Webhook source fact передаётся в H7 как `ExternalEvent` occurrence либо compatible extension source.

Webhook может привести только к результатам, разрешённым trigger revision и autonomy level:

```text
Observe  → observation/read-only Run
Suggest  → RunProposal
Prepare  → bounded preparation without commit
Execute  → policy-approved bounded action ceiling
```

Webhook никогда самостоятельно не выбирает autonomy level.

### 8.1 Запрет прямого Run mutation

Запрещён путь:

```text
webhook payload → UPDATE run/status
```

Нормативный путь:

```text
WebhookSourceFact
→ H7 TriggerEvaluation
→ H2 Authorization
→ RunProposal or CreateRun command
→ H1 application port
```

Для продолжения существующего Run требуется exact `RunContinuation` contract и durable cause. Произвольный `run_id` из payload не даёт право продолжения.

### 8.2 Human responses и approvals

Webhook не может удовлетворить H7 `HumanRequest` только строкой `approved=true`.

Typed response допускается лишь если endpoint revision специально связан с внешним authenticated principal/approval provider и response проходит:

- exact request binding;
- responder identity mapping;
- H2 eligibility/capability;
- operation fingerprint match;
- one-time continuation proof;
- schema validation;
- current request state check.

Стандартный webhook profile не предоставляет approval semantics.

## 9. Outbound processing pipeline

Нормативный pipeline:

```text
canonical source event
→ subscription match
→ transactional delivery intent
→ destination and classification policy
→ bounded versioned payload rendering
→ secret/redaction scan
→ H8 signing operation
→ HTTP delivery attempt
→ durable observation
→ retry, success, failure or reconciliation
→ H10 audit and metrics
```

### 9.1 Source event

Outbound subscription ссылается только на durable source event/fact владельца Horizon.

Telemetry log line, mutable UI state, database polling result без canonical reference и model-generated statement не являются допустимым source event.

### 9.2 Subscription filter

Filter может использовать только bounded fields:

- stable event kind;
- workspace-scoped resource kind;
- classification;
- outcome/status enum;
- explicit tags/labels из закрытого contract;
- source owner;
- approved subject reference.

Запрещены arbitrary SQL, raw payload regex, user-content matching без governed derived field и model execution внутри filter.

### 9.3 Payload rendering

Каждый outbound event contract имеет:

```text
webhook_event_kind
payload_schema_version
source_mapping_revision
canonicalization_revision
classification_ceiling
maximum_bytes
```

Payload включает минимально необходимое содержание. По умолчанию используются IDs, stable kinds, timestamps, bounded outcomes и short-lived authorized references вместо raw content.

Artifact bytes и sensitive excerpts не включаются без отдельного H6 export decision.

### 9.4 Durable delivery intent

Delivery intent создаётся в той же transaction или existing durable outbox, где source owner фиксирует факт, требующий уведомления.

Если subscription matching асинхронный, cursor и dedup должны гарантировать, что один source event создаёт не более одной logical delivery для exact subscription revision.

Unique logical key:

```text
(subscription_revision_id, source_event_id, payload_schema_version)
```

### 9.5 Signing

Outbound request подписывается H8-backed operation.

Signature обязана связывать:

- webhook event kind;
- payload schema version;
- delivery ID;
- timestamp;
- body hash;
- destination/endpoint revision where protocol requires.

Private key или HMAC secret не помещаются в work payload, PostgreSQL ordinary rows, logs или model-visible context.

### 9.6 Delivery semantics

Сетевая доставка является at-least-once.

Receiver должен deduplicate по immutable `delivery_id` или declared idempotency key.

Exactly-once не обещается, потому что Vestrace не контролирует transaction boundary получателя.

### 9.7 Response handling

Response body:

- ограничен maximum bytes;
- не интерпретируется как instruction;
- не попадает в prompt;
- сохраняется только как bounded status/receipt hash либо quarantined Artifact при явной policy;
- не может изменить source event.

Success определяется exact protocol profile, обычно 2xx плюс optional receipt validation.

Redirects отключены по умолчанию. Разрешённые redirects требуют отдельной destination policy и повторной SSRF/allowlist проверки каждого hop.

## 10. Network security и SSRF boundary

Outbound destination создаётся и утверждается principal/admin, а не моделью или payload.

Destination validation включает:

- HTTPS по умолчанию;
- explicit development exception для loopback/local testing;
- hostname allowlist;
- scheme/port allowlist;
- DNS resolution policy;
- блокировку link-local, metadata, multicast, broadcast и private ranges, если endpoint не утверждён как private destination;
- защиту от DNS rebinding;
- connection timeout;
- response timeout;
- maximum response bytes;
- redirect policy;
- TLS verification;
- optional certificate pinning/mTLS profile.

Модель может выбрать только заранее активированный endpoint ID в пределах capability. Она не передаёт произвольный URL.

## 11. Retry и idempotency

### 11.1 Классификация результата

```text
ConfirmedSucceeded
ConfirmedRejected
RetryableNotDelivered
PossiblyDelivered
PermanentFailure
```

Примеры:

- DNS failure до соединения → `RetryableNotDelivered`;
- connect timeout без передачи body → `RetryableNotDelivered`;
- timeout после передачи body → `PossiblyDelivered`;
- 429/503 с допустимым Retry-After → retryable;
- validated 2xx receipt → confirmed success;
- schema/auth 4xx → permanent failure, кроме exact protocol exceptions.

### 11.2 Retry policy

Retry policy задаёт:

```text
maximum_attempts
maximum_elapsed_time
initial_backoff
maximum_backoff
jitter_policy
retryable_categories
receiver_idempotency_requirement
```

Retry не может расширить original authorization, payload scope или destination revision.

### 11.3 Ambiguous completion

Если body мог быть принят получателем, но response потерян:

```text
status = OutcomeUnknown
```

Дальнейшее поведение:

1. если receiver поддерживает idempotency по delivery ID, повтор допускается policy;
2. если receiver поддерживает status/readback, выполняется reconciliation;
3. если receiver не поддерживает ни idempotency, ни reconciliation, автоматический retry external commitment запрещён;
4. delivery переводится в manual review/dead letter согласно policy.

### 11.4 Retry-After

`Retry-After` считается untrusted hint и ограничивается configured bounds. Он не может назначить бесконечную задержку или обойти maximum elapsed time.

## 12. Inbound response semantics

Inbound endpoint отвечает быстро после durable receipt/source-fact commit.

Рекомендуемые classes:

```text
2xx accepted or duplicate
4xx malformed/auth/replay/policy rejection
413 payload too large
415 unsupported media type
429 bounded rate/storm protection
5xx temporary internal unavailability before durable commit
```

После durable commit последующий trigger failure не меняет HTTP response на failure повторной доставки. Иначе отправитель мог бы бесконечно повторять уже принятую occurrence.

Detailed policy/auth reasons наружу не раскрываются, если это создаёт oracle.

## 13. Rate limits и storm protection

Inbound limits:

- requests per endpoint/window;
- bytes per window;
- concurrent body processing;
- invalid-signature threshold;
- duplicate/conflict threshold;
- trigger occurrence ceiling;
- Run/proposal ceiling;
- causal-loop detection;
- automatic suspension.

Outbound limits:

- deliveries per destination/window;
- concurrent connections;
- retry budget;
- bytes per window;
- failure threshold;
- dead-letter threshold;
- workspace/deployment quotas.

Storm protection state является durable и restart-safe.

## 14. Causal-loop prevention

Outbound payload включает optional bounded causation metadata:

```text
vestrace_delivery_id
source_event_id
causal_chain_id
hop_count
```

Inbound endpoint, если связан с тем же integration, проверяет causal chain.

Policy задаёт maximum hop count и запрещает циклы вида:

```text
Vestrace event
→ outbound webhook
→ external echo
→ inbound trigger
→ identical Vestrace event
→ ...
```

External system не может самостоятельно выбрать trusted causal chain identity без signature binding.

## 15. Endpoint lifecycle

### 15.1 Activation

Activation требует:

- validated immutable revision;
- H2 policy decision;
- required capability;
- H8 key/secret readiness;
- endpoint ownership;
- network policy validation;
- schema registry availability;
- test challenge или dry-run where supported;
- H10 audit fact.

### 15.2 Rotation

Secret/signing-key rotation создаёт новую H8 key revision и endpoint revision либо versioned accepted-key set.

Grace period:

- bounded;
- явно записан;
- не допускает algorithm downgrade;
- старый key имеет exact expiration;
- receipts фиксируют key revision, использованную при verification.

### 15.3 Suspension

Endpoint автоматически приостанавливается при:

- аномальном количестве invalid signatures;
- replay/conflict storm;
- repeated permanent failures;
- destination policy violation;
- secret/key revocation;
- classification leak detection;
- operator action;
- deployment emergency policy.

Suspension не удаляет history и не отменяет уже подтверждённые source facts.

### 15.4 Revocation и deletion

Revocation запрещает новые requests/deliveries. Pending deliveries переводятся согласно policy в cancelled/dead-letter.

Logical deletion сохраняет content-free audit и historical revision references. Secret material удаляется через H8 lifecycle.

## 16. Secrets и credentials

Webhook domain records содержат только opaque H8 references:

```text
verification_key_ref
signing_key_ref
client_certificate_ref
authentication_profile_ref
```

Запрещены:

- raw HMAC secrets;
- private keys;
- OAuth refresh/access tokens;
- client certificate private key;
- passwords;
- complete Authorization headers;
- secret values в URL query;
- model-visible credential handles, пригодные для использования вне broker.

H8 выдаёт bounded verification/signing operation или credential lease, привязанный к endpoint revision и purpose.

## 17. Data classification и privacy

Endpoint revision имеет `classification_ceiling`.

Inbound:

- source payload получает classification не ниже configured source class;
- payload не становится Public из-за внешней подписи;
- sensitive payload использует H6 quarantine/retention;
- PersonalData/Credentials labels применяются до trigger/context use.

Outbound:

- source classification сопоставляется destination policy;
- destination не получает данные выше своего ceiling;
- payload минимизируется;
- raw prompts, hidden reasoning, unrestricted tool output и secrets запрещены;
- Artifact links short-lived и отдельно authorized.

## 18. Capture profiles

State capture profiles применяются к webhook diagnostics:

- `Minimal` — hashes, kinds, timings, disposition;
- `Operational` — bounded retry/latency/error categories;
- `Reproducible` — redacted request/response capture только при policy;
- `Forensic` — Full capture ceiling лишь при explicit diagnostic authorization.

Ни один profile не сохраняет raw secret headers, private keys или hidden reasoning.

Optional bytes принадлежат H6 Artifact lifecycle.

## 19. Event schema compatibility

Webhook source facts, delivery intents и public delivery events используют stable kind + per-kind schema version.

Изменение wire payload создаёт новую schema version. Изменение смысла создаёт новый event kind.

Unknown future version:

- не интерпретируется как известная;
- не запускает trigger;
- не рендерится в outbound payload эвристически;
- может быть сохранена в quarantine для inspection;
- получает stable compatibility outcome.

Historical webhook receipts и deliveries не переписываются upcasters.

## 20. Public/API surfaces

H11 может предоставить:

```text
POST   /v1/webhook-endpoints
GET    /v1/webhook-endpoints
GET    /v1/webhook-endpoints/{id}
POST   /v1/webhook-endpoints/{id}/revisions
POST   /v1/webhook-endpoints/{id}:activate
POST   /v1/webhook-endpoints/{id}:disable
POST   /v1/webhook-endpoints/{id}:suspend
POST   /v1/webhook-endpoints/{id}:revoke
GET    /v1/webhook-receipts
GET    /v1/webhook-deliveries
POST   /v1/webhook-deliveries/{id}:retry
POST   /v1/webhook-deliveries/{id}:reconcile
```

Inbound receiver route использует отдельный opaque path namespace и не является authenticated management API.

Public adapters:

- не принимают arbitrary SQL;
- не принимают raw secret в ordinary JSON after initial secure setup flow;
- не позволяют модели создавать destination URL;
- не возвращают secret values;
- не меняют delivery status без owning application service;
- используют expected revision/idempotency keys.

## 21. UI semantics

UI показывает:

- direction;
- lifecycle state;
- active revision;
- last valid receipt/delivery;
- error category;
- signing profile;
- classification ceiling;
- trigger/subscription binding;
- retry/dead-letter state;
- suspension reason category;
- key rotation status без secret values.

UI обязана различать:

```text
Webhook received
Source fact persisted
Trigger evaluated
Run proposal created
Run created
Delivery attempted
Delivery confirmed
Delivery outcome unknown
```

Эти состояния нельзя сворачивать в одно «успешно».

## 22. Audit

H10 security audit фиксирует:

- endpoint creation/revision/activation;
- permission diff;
- key reference binding/rotation/revocation;
- accepted/rejected inbound receipt category;
- source fact creation;
- trigger evaluation reference;
- outbound subscription change;
- delivery intent creation;
- attempt/result/retry/reconciliation;
- manual override;
- suspension/revocation/deletion.

Audit содержит content-free facts, hashes и references. Raw payload, signature, Authorization header и response body не входят в audit record.

## 23. Observability

Bounded metrics:

- inbound requests by disposition;
- signature failures;
- replay rejections;
- duplicate/conflict rate;
- payload-size class;
- trigger evaluation latency;
- outbound queue age;
- attempt latency;
- success/retry/permanent/unknown counts;
- dead-letter count;
- endpoint suspension count.

Metric labels не содержат URL, hostname, event ID, workspace name, payload fields или external error text.

## 24. Failure semantics

### 24.1 H8 unavailable

Inbound request, требующий signature verification, отклоняется fail closed до source fact commit.

Outbound delivery не выполняется; intent остаётся pending/retryable в пределах policy.

### 24.2 PostgreSQL unavailable

Inbound request не считается принятым, если receipt/source fact не зафиксирован durable.

Outbound worker не отправляет request без durable delivery intent и acquired work lease.

### 24.3 H7 trigger worker unavailable

Source fact остаётся durable; trigger evaluation work возобновляется после восстановления.

### 24.4 H10 telemetry unavailable

Best-effort telemetry может быть потеряна, но mandatory audit intent остаётся durable согласно H10 boundary.

### 24.5 H6 quarantine unavailable

Large/sensitive inbound payload, которому требуется H6 storage, не сохраняется в ordinary logs или временный небезопасный backend. Request получает безопасную ошибку либо quarantine-pending только если bytes гарантированно сохранены approved store.

### 24.6 Schema registry unavailable

Новая inbound occurrence не интерпретируется. Outbound payload не рендерится без exact schema. Никакой fallback к unversioned JSON не допускается.

## 25. Security threats и controls

| Threat | Control |
|---|---|
| Forged inbound request | versioned signature/mTLS profile, H8 key boundary |
| Replay attack | timestamp, nonce/event ID, durable dedup window |
| Duplicate execution | source-fact dedup + H7 occurrence idempotency |
| Event-ID collision with altered body | body-hash conflict quarantine |
| SSRF | approved endpoint revisions, DNS/IP policy, no model URLs |
| DNS rebinding | resolution/connect validation and policy |
| Redirect escape | redirects disabled or every hop revalidated |
| Signature downgrade | exact algorithm/version binding |
| Secret leakage | opaque refs, header minimization, capture prohibition |
| Prompt injection | payload remains untrusted; no instruction elevation |
| Direct Run mutation | H7 trigger + H2 + H1 application path only |
| Approval spoofing | typed H7/H2 binding, not generic webhook field |
| Retry duplicate commitment | receiver idempotency/reconciliation requirement |
| Causal webhook loop | causal chain + hop ceiling + storm protection |
| Payload bomb | wire/decoded/depth/expansion limits |
| Error oracle | bounded indistinguishable external errors |
| Cross-workspace confusion | workspace-bound endpoint and RLS |

## 26. Acceptance scenarios

### Scenario A: Valid signed inbound event

Given active endpoint revision, valid signature, fresh timestamp and new event ID:

- receipt and source fact commit once;
- H7 occurrence is scheduled;
- duplicate request does not create a second occurrence;
- payload remains untrusted external content.

### Scenario B: Invalid signature

- no source fact;
- no trigger evaluation;
- bounded rejected receipt/audit fact;
- no detailed key oracle in response.

### Scenario C: Replay

- previously accepted event ID inside/outside replay window is recognized;
- identical duplicate is safe;
- altered body with same ID is quarantined/rejected;
- no second Run/proposal.

### Scenario D: Payload too large

- request rejected before unbounded parsing;
- raw body not written to logs;
- no source fact unless approved H6 quarantine succeeded.

### Scenario E: Direct Run instruction in payload

Payload contains `run_id` and `status=completed`:

- fields remain external data;
- no Run mutation occurs;
- only configured H7 mapping may create a bounded proposal/continuation request.

### Scenario F: Outbound successful delivery

- one durable intent;
- signed versioned payload;
- validated success receipt;
- source event unchanged;
- delivery marked confirmed.

### Scenario G: Timeout after request body sent

- attempt becomes `PossiblyDelivered`;
- delivery becomes `OutcomeUnknown`;
- automatic retry depends on receiver idempotency/reconciliation support;
- no blind duplicate external commitment.

### Scenario H: Receiver without idempotency

For an effectful notification where outcome is unknown:

- no automatic retry;
- dead-letter/manual review according to policy;
- diagnostic evidence preserved.

### Scenario I: Destination DNS changes to forbidden range

- connect-time validation rejects request;
- delivery remains failed/policy-blocked;
- no metadata-service access.

### Scenario J: Endpoint secret rotation

- new revision/key becomes active;
- old key accepted only during explicit grace window;
- receipts identify validation key revision;
- expiry removes old acceptance.

### Scenario K: Endpoint disabled by default

- newly created endpoint remains Draft/Disabled;
- no inbound route acceptance or outbound delivery until explicit activation gate.

### Scenario L: Trigger worker restart

- accepted source fact survives;
- occurrence evaluated once after restart;
- sender does not need to resend accepted request.

## 27. Contract tests будущей реализации

Будущий implementation plan обязан предусмотреть отдельные tests для:

- signature vectors and algorithm downgrade;
- proxy/IP allowlist handling;
- timestamp/replay boundaries;
- dedup conflict;
- payload bounds and decompression ratio;
- atomic receipt/source-fact/outbox persistence;
- trigger non-direct-mutation invariant;
- approval spoof rejection;
- outbound source-event dedup;
- signing-key isolation;
- SSRF, DNS rebinding and redirects;
- timeout-before/after-send classification;
- receiver idempotency/reconciliation;
- dead-letter lifecycle;
- causal-loop protection;
- classification/export filtering;
- secret/capture boundary;
- RLS and cross-workspace denial;
- restart recovery;
- unknown schema behavior.

## 28. Rollout

Безопасный порядок будущего rollout:

1. domain types и immutable revisions;
2. H8 verification/signing ports;
3. persistence, RLS, dedup и lifecycle;
4. inbound validation without trigger activation;
5. audit/telemetry;
6. H7 source-fact integration in observe-only mode;
7. outbound intent builder without network delivery;
8. loopback/fixture delivery;
9. admin-only activation;
10. bounded production canary;
11. higher autonomy only after separate H2/H7 review.

Readers/validators для новой schema revision развёртываются раньше producers.

## 29. Documentation-only boundary

Этот design не разрешает:

- создавать implementation branch;
- менять Rust source;
- создавать SQL migrations;
- добавлять HTTP routes;
- открывать ports;
- создавать H8 secrets/keys;
- выполнять network requests;
- активировать endpoint;
- менять H7 trigger behavior;
- менять public API;
- добавлять dependencies;
- изменять CI;
- начинать следующий amendment без отдельного review gate.

## 30. Итоговый инвариант

> **Webhook в Vestrace — это проверяемый внешний transport adapter. Inbound сторона может создать только durable untrusted source fact для H7; outbound сторона может доставить только bounded representation уже зафиксированного source event. Ни одна сторона не получает прямой authority над Run или другими доменными агрегатами.**
