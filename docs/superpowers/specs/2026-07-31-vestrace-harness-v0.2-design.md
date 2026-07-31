# Vestrace Harness v0.2 — архитектурная спецификация

**Статус:** утверждено пользователем, ожидает финального просмотра письменной спецификации  
**Дата:** 2026-07-31  
**Репозиторий:** `venm1r/vestrace`  
**Базовая спецификация:** `docs/superpowers/specs/2026-07-31-vestrace-v0.1-design.md`  
**Продукт:** self-hosted, model-agnostic, memory-native harness для универсальных AI-агентов

## 1. Назначение документа

Этот документ определяет следующий продуктовый слой Vestrace поверх v0.1: полноценную среду исполнения универсальных AI-агентов. Он не отменяет memory-first архитектуру v0.1, а добавляет долговременный runtime, планирование, инструменты, подагентов, политики, проверки, триггеры, артефакты и пользовательские каналы.

Vestrace v0.1 остаётся самостоятельным сервером памяти и когнитивных активов. Harness v0.2 использует его через стабильные application-порты и не встраивает orchestration-логику внутрь агрегатов памяти.

Положения v0.1 о том, что Vestrace не исполняет workflow и не декомпозирует произвольные задачи, остаются верны для релиза v0.1, но снимаются для v0.2 в границах этой спецификации.

## 2. Продуктовое определение

Vestrace Harness — локально разворачиваемая среда, которая:

1. принимает произвольную пользовательскую или системную цель;
2. выбирает профиль агента, навыки, workflow и модели;
3. строит проверяемый план;
4. исполняет шаги через разрешённые инструменты и изолированные среды;
5. делегирует ограниченные подзадачи специализированным подагентам;
6. сохраняет авторитетное состояние, checkpoints и журнал выполнения;
7. требует подтверждение для опасных действий;
8. проверяет результат по явным критериям;
9. создаёт доказуемые артефакты и кандидаты долговременной памяти;
10. может продолжить Run после рестарта, ожидания пользователя или временного сбоя.

Краткая позиция продукта:

> Vestrace — self-hosted, model-agnostic и memory-native runtime для долговременных универсальных AI-агентов.

Нормативный принцип:

> Модель предлагает действия. Harness управляет выполнением. Память сохраняет опыт. Политики определяют допустимое. Sandbox изолирует последствия.

## 3. Цели v0.2

Harness v0.2 должен доказать полный универсальный цикл:

```text
цель пользователя
→ AgentRuntimeSnapshot
→ план
→ проверка плана
→ model/tool/subrun execution
→ approvals и checkpoints
→ verification
→ outcome и artifacts
→ consolidation в memory candidates
```

Обязательные свойства:

- универсальный Coordinator, не привязанный к coding или research;
- один уровень контролируемых подагентов по умолчанию;
- динамические версионируемые планы в детерминированных рамках;
- единый Tool Runtime со сменными execution adapters;
- durable `AgentRun`, append-only execution journal и checkpoints;
- pause, resume, cancel и устойчивое ожидание input/approval;
- риск-ориентированная проверка результатов;
- версионируемые triggers с ограниченной автономностью;
- многоуровневый context system;
- централизованный Policy Engine;
- policy-constrained Model Router;
- Connector Registry и Credential Broker;
- content-addressed Artifact Store;
- разделённые Control, Worker и Execution planes;
- внешние контрактные расширения;
- композиция Agent Profiles, Skills, Workflows и Packages;
- иерархические бюджеты и квоты;
- observability, audit и evaluation system;
- HTTP API, durable event stream, MCP и SDK;
- Personal, Team и Embedded self-hosted deployment.

## 4. Явно не входит в v0.2

В первый harness-релиз не входят:

- неограниченная автономность;
- `Commit` как стандартный уровень автономности;
- глубокие рекурсивные сети агентов;
- глобальная общая blackboard-память;
- автоматическая установка расширений моделью;
- автоматическое создание и активация Skills моделью;
- автоматическое изменение policies или постоянных capabilities;
- публичный marketplace;
- managed SaaS, биллинг и глобальный cloud control plane;
- Kubernetes, Kafka, NATS или service mesh как обязательные зависимости;
- cross-region active-active;
- собственный distributed consensus;
- полноценный browser operator;
- десятки встроенных email, calendar и CRM connectors;
- хранение скрытого chain-of-thought;
- автоматическое обучение router или prompts без утверждения;
- автоматический execution replay опасных действий;
- бесконечно живущие агенты без явного lifecycle Run.

Архитектура не должна блокировать эти возможности позднее.

## 5. Архитектурные принципы

### 5.1 Memory-first separation

Memory Plane остаётся самостоятельным. Runtime получает память через application services и не изменяет memory tables напрямую.

### 5.2 Run-first execution

Авторитетной единицей работы является `AgentRun`, а не chat thread, HTTP request или процесс worker.

### 5.3 Deny by default

Любое действие, чтение данных, делегирование, экспорт или использование модели требует положительного решения политики.

### 5.4 Immutable revisions

Agents, Skills, Workflows, Plans, Policies, Models, Tools, Extensions и Artifacts используют неизменяемые ревизии. Активный Run фиксирует точные revisions.

### 5.5 Durable state over model memory

Продолжение задачи определяется состоянием harness, а не попыткой модели восстановить ход работы из переписки.

### 5.6 Deterministic checks first

Когда результат можно проверить программно или чтением внешнего состояния, model judge не является главным доказательством.

### 5.7 External data is untrusted

Tool output, web content, email, документы и webhook payload считаются данными, а не инструкциями runtime.

### 5.8 Modular monolith first

Одна кодовая база и общие контракты, но несколько независимо запускаемых ролей. Распределённость добавляется без изменения доменной модели.

## 6. Архитектурные planes

```text
Vestrace
├── Memory Plane
│   ├── events
│   ├── memories
│   ├── retrieval
│   ├── context packs
│   └── provenance
│
├── Control Plane
│   ├── conversations and interactions
│   ├── run coordinator
│   ├── plan validator
│   ├── policy engine
│   ├── model router
│   ├── trigger scheduler
│   └── administration
│
├── Worker Plane
│   ├── model workers
│   ├── tool workers
│   ├── subrun workers
│   ├── artifact workers
│   ├── evaluation workers
│   └── memory consolidation workers
│
├── Execution Plane
│   ├── tool runtime
│   ├── sandbox manager
│   ├── Docker runners
│   ├── connector adapters
│   └── remote execution providers
│
├── Storage Plane
│   ├── PostgreSQL + pgvector
│   ├── Artifact Store
│   └── Secret backend
│
└── Evaluation Plane
    ├── execution journal
    ├── security audit
    ├── evaluation datasets
    ├── regression gates
    └── replay and diagnostics
```

### 6.1 Роли бинарника

Один Rust workspace предоставляет минимум следующие команды:

```text
vestrace all
vestrace server
vestrace worker
vestrace scheduler
vestrace sandbox-manager
vestrace mcp
vestrace migrate
vestrace doctor
vestrace schema <http|events|mcp|extensions>
```

`vestrace all` запускает локальную конфигурацию. Раздельные роли используют те же application contracts.

## 7. Универсальная модель агента

Vestrace не создаёт одного огромного агента с глобальным prompt. Поведение формируется композицией:

```text
AgentProfile
+ Skill revisions
+ Workflow revision
+ RoutingProfile revision
+ Policy bundle revisions
+ MemoryProfile revision
+ Tool catalogue snapshot
= AgentRuntimeSnapshot
```

Reference profiles:

- `universal-coordinator`;
- `research-specialist`;
- `data-analyst`;
- `document-writer`;
- `workspace-automation`;
- `coding-agent`;
- `verification-agent`;
- `personal-assistant`.

Coordinator является главным агентом Run. Он может создавать ограниченные `SubRun`, но не может расширять собственные права или подключать неизвестные инструменты.

## 8. AgentRun

### 8.1 Сущность

```text
AgentRun
├── workspace_id
├── objective
├── coordinator_snapshot_id
├── active_plan_revision_id
├── execution_mode
├── status
├── budgets
├── consumed_resources
├── current_step_id
├── checkpoint_id
├── result
├── run_version
├── lease_owner
├── lease_until
└── timestamps
```

### 8.2 Статусы Run

```text
Created
→ Preparing
→ Running
├── WaitingForInput
├── WaitingForApproval
├── WaitingForDependency
├── Paused
├── PausedPolicyChanged
├── Succeeded
├── SucceededWithWarnings
├── Partial
├── Failed
├── Cancelled
└── Expired
```

`WaitingForInput` и `WaitingForApproval` являются устойчивыми нормальными состояниями, а не ошибками.

### 8.3 RunStep

```text
RunStep
├── plan_step_reference
├── assigned_actor
├── input_references
├── status
├── attempt
├── model_decision_reference
├── tool_invocation_reference
├── output_artifacts
├── error
└── timing
```

Статусы шага:

```text
Pending
Ready
Running
Waiting
Succeeded
Failed
Skipped
Cancelled
Unknown
```

### 8.4 Завершение

`Succeeded` разрешён только после прохождения обязательных success criteria. При исчерпании бюджета или частичном выполнении создаётся `PartialOutcome`, а не ложный успех.

## 9. Планирование

### 9.1 Режимы

```text
Direct
Guided
Workflow
```

- `Direct` — простая задача без отдельного долговременного графа;
- `Guided` — модель предлагает план, harness валидирует и исполняет;
- `Workflow` — используется заранее утверждённый `WorkflowDefinition`.

### 9.2 ExecutionPlan

```text
ExecutionPlan
├── objective
├── assumptions
├── success_criteria
├── steps
├── dependencies
├── budgets
├── required_approvals
└── revision
```

Типы шагов:

```text
Reason
ToolCall
Delegate
HumanInput
HumanApproval
Evaluate
Synthesize
End
```

Каждый шаг объявляет input, output schema, зависимости, capabilities, tools, budget, timeout, retry policy и критерий завершения.

### 9.3 Валидация плана

`PlanValidator` обязан проверить:

1. отсутствие неограниченных циклов;
2. существование указанных agents, skills и tools;
3. совместимость полномочий;
4. общий бюджет;
5. необходимые approvals;
6. достижимость контекста;
7. наличие ожидаемого результата;
8. путь к завершению или контролируемой ошибке.

Невалидный план не исполняется. Модель получает структурированные validation errors.

### 9.4 Ревизии плана

Изменение плана создаёт новую неизменяемую ревизию. Выполненные шаги не переписываются. Ограничиваются `max_plan_revisions` и дополнительный бюджет пересмотра.

## 10. Делегирование и SubRun

### 10.1 Модель

По умолчанию используется:

```text
Coordinator
└── SubAgent
```

Глубина делегирования — один уровень. Вложенная делегация требует отдельной policy.

### 10.2 DelegationRequest

```text
DelegationRequest
├── parent_run_id
├── parent_step_id
├── target_agent_revision
├── objective
├── expected_output_schema
├── delegated_context
├── allowed_tools
├── capability_ceiling
├── memory_scope
├── resource_allocation
└── completion_policy
```

Подагент не получает полный parent transcript, secrets, parent scratchpad или неиспользуемые tools.

### 10.3 Эффективные полномочия

```text
Coordinator permissions
∩ SubAgent definition
∩ DelegationRequest
∩ Workflow constraints
∩ Policy Engine decision
```

Подагент не может повысить budget, расширить tools, прочитать дополнительную память или изменить parent run напрямую.

### 10.4 HandoffArtifact

```text
HandoffArtifact
├── summary
├── structured_output
├── supporting_sources
├── confidence
├── unresolved_questions
├── warnings
└── produced_artifacts
```

Coordinator может принять, отклонить, уточнить или отправить результат на независимую проверку.

## 11. Tool Runtime

### 11.1 Единый контракт

Модель видит единый каталог tools независимо от транспорта.

```text
ToolDefinition
├── stable_id
├── revision
├── name
├── description
├── input_schema
├── output_schema
├── required_capabilities
├── risk_classification
├── side_effect_classification
├── idempotency_mode
├── timeout_policy
├── retry_policy
├── data_policy
└── execution_binding
```

Execution adapters:

- MCP;
- HTTP;
- native Rust;
- sandbox;
- browser adapter;
- human action;
- subagent.

Browser adapter является контрактом расширения; полноценный browser operator не входит в v0.2.

### 11.2 Побочные эффекты

```text
ReadOnly
IdempotentWrite
NonIdempotentWrite
Destructive
Irreversible
ExternalCommitment
```

Risk levels:

```text
Low
Moderate
High
Critical
```

Risk и side effects определяются ToolDefinition и policy, а не названием tool или текстом модели.

### 11.3 Двухфазные действия

Для важных операций:

```text
Prepare
→ Preview
→ Approve
→ Commit
→ Verify
```

Approval привязывается к хешу точной операции: tool ID, revision, normalized arguments, principal и run ID.

### 11.4 ToolInvocation

```text
ToolInvocation
├── run_id
├── step_id
├── tool_revision
├── normalized_arguments
├── authorization_decision
├── approval_reference
├── execution_environment
├── idempotency_key
├── output_artifacts
├── side_effects
├── verification_result
└── status
```

Статусы:

```text
Prepared
AwaitingApproval
Executing
Succeeded
Failed
Unknown
Cancelled
Compensated
```

`Unknown` обязателен, когда внешний сервис мог выполнить действие, но ответ не получен.

### 11.5 Retry и reconciliation

- read-only обычно повторяется безопасно;
- idempotent write повторяется с тем же стабильным ключом;
- non-idempotent write требует проверки состояния;
- irreversible действие автоматически не повторяется;
- unknown completion сначала проходит reconciliation.

Compensation является отдельным инструментом или workflow и не считается универсальным rollback.

## 12. Execution environments и sandbox

Типы сред:

```text
None
EphemeralWorkspace
PersistentWorkspace
IsolatedContainer
RemoteExecution
```

Каждая среда задаёт mounts, network policy, secrets policy, CPU, RAM, storage, lifetime, export и cleanup.

`SandboxProvider` является application port. Начальные реализации:

- `NoSandbox`;
- `LocalProcess` только для доверенных сценариев;
- Docker как основной изолированный runner;
- future remote provider.

Сетевые режимы:

```text
DenyAll
AllowListedDomains
ProviderManaged
Unrestricted
```

`Unrestricted` требует отдельной policy и не является default.

Входные artifacts монтируются read-only. Export выполняется только для явно выбранных output paths после validation и secret scan.

## 13. Durable state, checkpoints и recovery

### 13.1 Execution journal

Append-only `RunEvent` содержит доменные события:

```text
run.created
run.started
plan.proposed
plan.validated
plan.revised
step.started
model.selected
model.completed
tool.prepared
tool.approved
tool.completed
delegation.created
subrun.completed
checkpoint.created
run.paused
run.resumed
run.completed
run.failed
```

### 13.2 Checkpoint

```text
RunCheckpoint
├── run_version
├── active_plan_revision
├── completed_steps
├── ready_steps
├── pending_tool_calls
├── pending_approvals
├── active_subruns
├── context_references
├── artifact_references
├── budget_counters
└── resume_cursor
```

Checkpoint создаётся после шага, перед ожиданием, перед опасным действием, после смены плана, перед остановкой worker и периодически в длинном цикле.

### 13.3 Concurrency

Изменение Run использует optimistic concurrency:

```text
expected_run_version = N
```

Worker ownership использует lease и heartbeat. Два worker не могут одновременно продолжить один Run.

### 13.4 Resume

Resume:

1. загружает Run;
2. получает lease;
3. загружает checkpoint;
4. сверяет незавершённые операции;
5. восстанавливает бюджеты;
6. проверяет approvals и policies;
7. пересобирает ограниченный context envelope;
8. продолжает с resume cursor.

### 13.5 Pause и cancel

Pause прекращает выдачу новых шагов, сохраняет checkpoint и освобождает lease. Cancel не утверждает, что уже совершённые внешние действия отменены; компенсации выполняются только если явно определены.

### 13.6 Replay

- logical/state replay не вызывает внешние systems;
- model replay использует отдельную policy;
- tool replay по умолчанию использует mocks;
- write actions не повторяются в production replay.

## 14. Проверка и самокоррекция

### 14.1 SuccessCriteria

```text
SuccessCriteria
├── required_outputs
├── output_schema
├── deterministic_assertions
├── evidence_requirements
├── quality_threshold
├── prohibited_outcomes
└── completion_policy
```

### 14.2 Уровни проверки

```text
None
Basic
Standard
Strict
Critical
```

- `Basic`: schema и обязательные поля;
- `Standard`: полнота, evidence, contradictions, deterministic tests;
- `Strict`: независимый verifier;
- `Critical`: независимая проверка, human approval и external state verification.

Приоритет доказательств:

```text
1. фактическое состояние внешней системы
2. детерминированные тесты
3. формальные правила и schemas
4. проверка источников
5. verifier model
6. самооценка producer model
```

### 14.3 EvaluationReport

```text
EvaluationReport
├── target
├── criteria_results
├── deterministic_checks
├── evidence_coverage
├── contradictions
├── confidence
├── severity
├── recommendation
└── evaluator_provenance
```

Рекомендации:

```text
Accept
AcceptWithWarnings
Revise
Retry
RequestEvidence
RequestHumanInput
Reject
```

### 14.4 Ограничение correction loop

Ограничиваются attempts, verification rounds, plan revisions и additional cost. После исчерпания лимита Run становится `Partial`, `WaitingForInput` или `Failed`.

## 15. Triggers и автономность

### 15.1 TriggerDefinition

```text
TriggerDefinition
├── stable_id
├── revision
├── trigger_kind
├── source_binding
├── filter
├── target_agent_profile
├── objective_template
├── initial_context_mapping
├── autonomy_level
├── budgets
├── cooldown
├── deduplication_policy
└── enabled_status
```

Trigger kinds:

```text
Manual
API
Schedule
ExternalEvent
StateChange
Condition
RunContinuation
```

В первом vertical slice обязательны `Manual`, `API`, `Schedule` и `RunContinuation`. External event adapters поддерживаются архитектурно и могут поставляться отдельными extensions.

### 15.2 Уровни автономности

```text
Observe
Suggest
Prepare
Execute
Commit
```

`Commit` отключён по умолчанию и не входит в стандартный v0.2 profile.

- `Observe` анализирует событие;
- `Suggest` создаёт предложение;
- `Prepare` выполняет безопасную подготовку и preview;
- `Execute` выполняет разрешённые обратимые действия;
- `Commit` разрешает заранее утверждённые внешние обязательства и требует отдельного feature flag и policy.

### 15.3 RunProposal

До автозапуска trigger может создать предложение с objective, планом, tools, данными, стоимостью, риском и approvals.

### 15.4 Защита от штормов

Обязательны deduplication keys, cooldown, max runs per window, concurrency ceiling, daily cost ceiling и causal-loop protection.

## 16. Context system и память

### 16.1 Слои

```text
Context System
├── Identity Context
├── Long-Term Memory
├── Run State
├── Working Context
├── Artifact Context
├── External Evidence
└── Prompt Assembly
```

### 16.2 Identity Context

Содержит AgentProfile revision, role, instructions, capabilities, prohibited actions, communication policy, model requirements и memory policy. Модель не может его изменять.

### 16.3 Long-Term Memory

Долговременная память хранит только устойчивые факты, предпочтения, ограничения, решения, процедуры и проверенные outcomes. Черновые планы, raw transcripts, временные ошибки и неподтверждённые гипотезы не активируются автоматически.

### 16.4 Working Context

Working context пересобирается для каждого model invocation из objective, plan fragment, constraints, selected memories, verified observations, tool schemas, artifact excerpts и output contract.

### 16.5 Structured scratchpad

Harness может хранить assumptions, open questions, candidate actions, calculations и risks в явных полях. Скрытый chain-of-thought не сохраняется и не требуется для воспроизводимости.

### 16.6 Evidence

```text
EvidenceItem
├── source
├── retrieval_time
├── content_hash
├── excerpt
├── authority_metadata
├── freshness
├── sensitivity
├── trust_classification
└── artifact_reference
```

Evidence проходит claim extraction, contradiction check и evaluation до создания memory candidate.

### 16.7 ModelContextEnvelope

```text
ModelContextEnvelope
├── identity_instructions
├── task_objective
├── active_constraints
├── plan_step
├── selected_memory
├── evidence
├── artifact_excerpts
├── available_tools
├── output_contract
├── remaining_budget
└── trust_annotations
```

Приоритет:

1. system safety and runtime policy;
2. AgentProfile;
3. user objective and confirmed constraints;
4. approved plan;
5. active authoritative memory;
6. verified execution results;
7. external evidence;
8. unverified observations.

### 16.8 ContextSnapshot

Каждый model invocation сохраняет source references, revisions, rendered hash, token accounting, context policy version, omitted reasons и redaction record. Capture mode: `Full`, `Redacted` или `MetadataOnly`.

### 16.9 Run consolidation

После завершения Run:

```text
outcome
→ decisions
→ reusable procedures
→ unresolved tasks
→ evidence evaluation
→ memory candidates
→ existing write policy
```

Harness не обходит memory write policies v0.1.

## 17. Conversations, interactions и channels

### 17.1 Separation

`Conversation` хранит коммуникацию. `AgentRun` хранит исполняемую работу. Один conversation может быть связан с несколькими runs.

### 17.2 InteractionEvent

```text
InteractionEvent
├── actor
├── channel
├── conversation
├── message_kind
├── content_parts
├── attachments
├── reply_reference
├── external_identifiers
├── occurred_at
├── trust_classification
└── idempotency_key
```

Kinds:

```text
UserMessage
SystemMessage
Clarification
ApprovalResponse
CancellationRequest
RunCommand
AttachmentProvided
ExternalNotification
Feedback
```

### 17.3 HumanRequest

```text
HumanRequest
├── run_id
├── request_kind
├── question
├── available_choices
├── expected_schema
├── reason
├── blocking_status
├── expires_at
└── continuation_token
```

Types: Clarification, MissingData, Choice, Approval, Review, AuthenticationRequired, ManualAction.

### 17.4 Channels

Начальные channel adapters:

- HTTP API;
- WebSocket или SSE;
- CLI;
- MCP client/server integration.

Email и messenger adapters подключаются позднее как extensions.

### 17.5 Streaming

Статус, plan updates, tool activity, artifacts, approvals и final output передаются отдельными event types. Streaming не является источником истины.

### 17.6 Участники

Run roles:

```text
Owner
Requester
Approver
Contributor
Observer
```

Присутствие в conversation не даёт право approve, cancel, export или increase budget.

## 18. Policy Engine

### 18.1 Централизованное решение

```text
Policy Enforcement Point
→ AuthorizationRequest
→ Policy Engine
→ Permit | Deny | PrepareOnly | RequireApproval | PermitWithObligations
```

### 18.2 AuthorizationRequest

```text
AuthorizationRequest
├── principal
├── acting_agent_revision
├── run_and_step
├── action
├── resource
├── normalized_arguments
├── data_classifications
├── tool_risk
├── execution_environment
├── delegation_lineage
├── approval_references
├── budget_state
└── current_time
```

### 18.3 Policy layers

```text
System
→ Deployment
→ Workspace
→ Principal
→ Agent
→ Workflow
→ Run
→ Delegation
→ Tool
```

Нижний слой не расширяет верхний ceiling. Конфликт решается в пользу более ограничительного результата.

### 18.4 Capabilities

Примеры:

```text
memory.read
memory.write_candidate
artifact.read
artifact.export
tool.invoke:<tool-id>
run.create
run.cancel
run.delegate
trigger.manage
approval.grant
secret.use
```

Право prepare не означает право commit.

### 18.5 Data classification

```text
Public
Internal
Confidential
Restricted
Secret
```

Labels: PersonalData, Credentials, Financial, Medical, Legal, SourceCode, CustomerData.

Policies определяют provider eligibility, delegation, export, retention, redaction и channel sharing.

### 18.6 ApprovalGrant

```text
ApprovalGrant
├── approver
├── operation_hash
├── permitted_action
├── resource
├── argument_constraints
├── maximum_executions
├── valid_from
├── valid_until
├── run_binding
└── revocation_status
```

Начальные modes: `Once` и `ForCurrentRun`.

### 18.7 Policy records

Каждое решение сохраняет request hash, result, matched rules, obligations, policy bundle revision, timestamp и explanation.

Policy bundles являются данными с tests. Модель не может изменять или активировать policy.

## 19. Model Runtime и Router

### 19.1 ModelDefinition

```text
ModelDefinition
├── stable_id
├── revision
├── provider_binding
├── provider_model_name
├── capabilities
├── modalities
├── context_limits
├── structured_output_support
├── tool_call_support
├── reasoning_modes
├── latency_class
├── quality_baselines
├── pricing_revision
├── data_handling_attributes
└── operational_status
```

### 19.2 Invocation requirements

```text
ModelInvocationRequest
├── task_type
├── required_capabilities
├── preferred_capabilities
├── input_modalities
├── output_contract
├── data_classifications
├── context_size_estimate
├── minimum_quality
├── maximum_latency
├── cost_ceiling
├── independence_requirement
└── fallback_policy
```

### 19.3 Selection order

1. operational availability;
2. policy eligibility;
3. required capabilities;
4. context and modality compatibility;
5. minimum quality;
6. budget compatibility;
7. ranking among eligible candidates.

### 19.4 Quality-first ranking

Используется утверждённая формула:

```text
observed_weight = min(observation_count / 20.0, 1.0)
baseline_weight = 1.0 - observed_weight
expected_quality =
    observed_weight × smoothed_task_type_quality
  + baseline_weight × configured_baseline_quality
```

Сначала применяется quality threshold, затем reliability, latency и cost ranking.

### 19.5 Fallback

Errors нормализуются как transient failure, rate limit, unavailable, context too large, unsupported capability, invalid structured output, safety refusal, policy denied или budget exceeded.

Safety refusal не обходится другой моделью автоматически без отдельного анализа причины.

### 19.6 Verifier independence

Policy может потребовать different model revision, family, provider и prompt template, а также запрет producer scratchpad.

### 19.7 Records и budget

Каждый вызов сохраняет candidates, exclusions, selected model revision, policy decision, context snapshot, provider request ID, usage, price revision, cost, latency и validation result.

## 20. Connector Registry и Credential Broker

### 20.1 ConnectorDefinition

Описывает authentication modes, resources, operations, external scopes, webhook capabilities, data handling и adapter binding. Секретов не содержит.

### 20.2 Connection

```text
Connection
├── workspace
├── owner_principal
├── connector_revision
├── external_identity
├── granted_external_scopes
├── credential_reference
├── status
├── sharing_policy
└── last_validation
```

Statuses: PendingAuthorization, Active, Degraded, ReauthorizationRequired, Revoked, Disabled, Expired.

Connections бывают user-bound и workspace-bound.

### 20.3 Credential Broker

Модель видит только logical connection ID. Broker выдаёт adapter ограниченный `CredentialLease`, привязанный к run, step/invocation, operations, resources, expiration, uses и policy decision.

Постоянные bearer tokens, passwords, private keys и refresh tokens не передаются модели и по умолчанию не помещаются в sandbox.

### 20.4 Delegation

Подагент получает отдельный `ConnectionAccessGrant`, ограниченный operations, resources, purpose, expiration и maximum invocations.

### 20.5 Secret controls

Секреты запрещены в prompts, context snapshots, ordinary events, artifacts, command line и model-visible tool arguments.

Перед persistence/export выполняются exact known-secret matching, pattern detection и redaction/quarantine policy.

## 21. Artifact system

### 21.1 Storage model

PostgreSQL хранит metadata, ownership, policy, lifecycle и provenance. Байты хранятся через `ArtifactStorePort`.

Начальные backends:

- `LocalContentAddressedStore`;
- S3-compatible adapter.

### 21.2 Artifact

```text
Artifact
├── stable_id
├── workspace
├── owner_principal
├── producing_run_and_step
├── kind
├── media_type
├── original_filename
├── byte_size
├── content_hash
├── storage_reference
├── sensitivity
├── lifecycle_status
├── provenance
└── timestamps
```

Artifact ID является логическим объектом. Content hash идентифицирует конкретные неизменяемые bytes.

### 21.3 Revisions и representations

Изменение создаёт новую ArtifactRevision. Производные representations включают ExtractedText, Markdown, Thumbnail, PageImage, AudioTranscript, StructuredTable, ArchiveManifest, CodeIndex, EmbeddingChunks, Summary, RedactedCopy и Preview.

Каждое representation указывает source revision и generator revision.

### 21.4 Ingestion

```text
upload
→ streaming hash
→ validation
→ quarantine
→ security inspection
→ immutable revision
→ available
```

Archive depth, expansion ratio, MIME mismatch, executable content и parser sandboxing ограничиваются policy.

### 21.5 Sandbox mounts и export

Входы read-only. Outputs становятся WorkingArtifacts. После validation, secret scan, policy decision и optional evaluation они могут стать UserDeliverables.

### 21.6 Retention и purge

Logical delete, retention expiration, legal hold, hard purge и blob garbage collection различаются. Hard purge удаляет representations, chunks, embeddings, cached excerpts, thumbnails, blobs и temporary export copies, сохраняя только минимальную запись факта purge без содержимого.

## 22. Runtime topology и очередь

### 22.1 Control Plane

Принимает requests, создаёт Run, проверяет policies, валидирует plan, планирует work, управляет approvals, выдаёт leases и собирает outcome. Не запускает user shell и не удерживает долгие HTTP requests.

### 22.2 Worker Plane

Work kinds:

```text
InvokeModel
InvokeTool
ExecuteSubRun
BuildRepresentation
EvaluateResult
ConsolidateMemory
DeliverNotification
ReconcileOperation
```

Worker является enforcement point и проверяет authorization reference.

### 22.3 PostgreSQL queue

Первая версия использует `FOR UPDATE SKIP LOCKED`. Изменение Run, RunEvent, WorkItem и outbox notification создаются в одной транзакции.

### 22.4 Worker registration

Workers объявляют work kinds, adapters, providers, resource limits, security labels, deployment zone и heartbeat. Scheduler назначает только совместимому worker.

### 22.5 Sandbox Manager

Отдельный порт управляет creation, mounts, network, credential proxy, limits, heartbeat, outputs и destruction. Он не формирует model prompts и не меняет цель Run.

### 22.6 Deployment modes

```text
Local:
  vestrace all + PostgreSQL + local artifact store + Docker

Split:
  server + scheduler + workers + PostgreSQL + S3-compatible store

Distributed later:
  replicated control plane + worker pools + remote sandboxes
```

## 23. Extensions

### 23.1 Категории

- Tool Provider;
- Connector;
- Channel Adapter;
- Model Provider;
- Artifact Processor;
- Sandbox Provider;
- Evaluator;
- Trigger Source;
- Notification Provider.

### 23.2 ExtensionManifest

Содержит ID, version, publisher, kinds, protocol version, capabilities, requested permissions, operations, config schema, secret requirements, network requirements, data declaration, health contract и integrity digest.

### 23.3 Isolation

Modes:

```text
TrustedInProcess
ExternalProcess
SandboxedProcess
RemoteService
```

Сторонние extensions по умолчанию `SandboxedProcess` или `RemoteService`. Rust dynamic libraries не являются публичным plugin contract.

### 23.4 Invocation

Extension получает normalized input, authorization reference, credential lease handles, allowed artifact handles, deadline, idempotency key и output contract. Write result должен содержать внешний operation ID или способ reconciliation, когда это возможно.

### 23.5 Trust и activation

Statuses: Installed, Disabled, Quarantined, Active, Degraded, Revoked, Incompatible.

Trust levels: Builtin, Verified, Approved, Unverified, Blocked.

Activation отделена от install. Permission diff, conformance tests и administrator approval обязательны при расширении полномочий.

### 23.6 SDK

- Rust SDK для встроенных adapters;
- language-neutral HTTP/gRPC protocol SDK;
- MCP bridge;
- declarative REST/OpenAPI connector subset.

## 24. Agent Profiles, Skills, Workflows и Packages

### 24.1 AgentProfile

Содержит role, objective class, identity instructions, communication policy, routing profile, memory policy, capability ceiling, default skills, workflow families, delegation policy, verification policy и lifecycle status.

### 24.2 SkillDefinition

Skill — переиспользуемая типизированная способность с activation conditions, instructions, tools, capabilities, schemas, memory pattern, model requirements, verification contract, examples и tests.

Skill не выдаёт permissions. Он только объявляет requirements.

### 24.3 WorkflowDefinition

Типизированный граф с nodes, transitions, budgets, policy requirements и completion criteria. Runtime state хранится в AgentRun.

Node kinds:

```text
ModelStep
ToolStep
DelegateStep
HumanInputStep
ApprovalStep
EvaluationStep
TransformStep
ConditionalStep
ParallelStep
JoinStep
EndStep
```

### 24.4 Snapshot

`AgentRuntimeSnapshot` закрепляет точные revisions profile, skills, workflow, routing, policies, memory profile, tool catalogue и extensions.

### 24.5 AgentPackage

Пакет содержит profiles, skills, workflows, policy templates, schemas, evaluation datasets, examples и integrity digest. Он не содержит secrets, active connections, approvals или runtime state.

### 24.6 Customization

Используются base profile + explicit overlays. Глубокое наследование запрещается. Overlay может только сохранять или сужать capability ceiling.

## 25. Бюджеты и квоты

### 25.1 Иерархия

```text
Deployment
→ Workspace
→ Principal
→ Trigger / AgentProfile
→ Run
→ Step / SubRun reservation
```

### 25.2 ResourceBudget

Категории: money, tokens, model calls, tool calls, steps, duration, sandbox CPU, RAM/storage, network, artifacts, subruns и parallelism.

Каждая категория может иметь soft, approval и hard limits.

### 25.3 Reservation accounting

Перед выполнением операция резервирует estimate, после чего actual usage reconciles и unused amount освобождается. Конкурентные steps не могут одновременно потратить один и тот же остаток.

### 25.4 SubRun allocation

Coordinator выделяет отдельный budget allocation. Подагент не видит и не использует невыделенный остаток.

### 25.5 Реакции на лимиты

```text
Continue
Degrade
Replan
RequestIncrease
Pause
TerminateWithPartialResult
```

Safety и mandatory quality criteria не понижаются. Budget increase требует типизированного запроса с ценностью, стоимостью и сокращённой альтернативой.

### 25.6 Quotas и fairness

Workspace quotas ограничивают concurrent/queued runs, sandboxes, request rate, storage, spend, trigger executions и notifications.

Scheduler использует weighted fairness с aging и приоритетами Interactive, High, Normal, Background, Maintenance.

### 25.7 Trigger ceilings

Autonomous triggers имеют более строгие budgets, cooldown, failure threshold и automatic suspension при аномалии.

## 26. Observability, audit и evaluations

### 26.1 Operational telemetry

OpenTelemetry-compatible logs, metrics и traces. Correlation chain:

```text
workspace_id
conversation_id
run_id
subrun_id
step_id
invocation_id
causation_id
```

Sensitive content не используется как telemetry labels.

### 26.2 Execution journal

Доменный append-only журнал отвечает на вопросы о plan, tools, models, approvals, revisions и reasons.

### 26.3 Security audit

Отдельно журналируются capability grants, policy activation, connection use, credential leases, approvals, commitments, artifact exports, memory purge, extension activation и autonomy changes.

Audit record содержит principal, agent, operation, resource, decision, obligations, external identity, result и integrity chain.

### 26.4 Privacy-aware capture

Modes:

```text
Full
Redacted
StructuredOnly
MetadataOnly
Disabled
```

Даже MetadataOnly сохраняет hashes, references, revisions, usage, validation и policy decision.

### 26.5 Evaluation system

EvaluationDataset описывает tasks, inputs, expected properties, prohibited outcomes, grading и revision.

EvaluationRun фиксирует tested component snapshot, environment, repetitions, grading и budgets.

Метрики включают task success, criteria pass, factual support, tool correctness, policy compliance, false success, cost per successful Run, human intervention, memory contamination и verifier disagreement.

### 26.6 Regression gates

Новые component revisions проходят targeted tests, eval suite, policy compliance и baseline comparison до activation.

Shadow mode не исполняет side effects. Canary допускается позднее только для безопасных сценариев.

## 27. Public API, event stream, MCP и SDK

### 27.1 HTTP API

Основной внешний контракт — versioned `/v1` HTTP API. Public DTO отделены от domain и SQL structures.

Core resources:

```text
/workspaces
/conversations
/interactions
/runs
/runs/{id}/steps
/runs/{id}/events
/runs/{id}/artifacts
/runs/{id}/approvals
/agents
/skills
/workflows
/tools
/triggers
/connections
/policies
/models
/extensions
/evaluations
/audit
```

### 27.2 Commands

State changes используют explicit endpoints: CreateRun, PauseRun, ResumeRun, CancelRun, SubmitHumanResponse, GrantApproval, RevisePlan, IncreaseBudget.

Не используется универсальный unrestricted PATCH состояния Run.

### 27.3 Async lifecycle

`POST /v1/runs` возвращает `202 Accepted`, run ID, status, version и URLs для events/outcome.

### 27.4 Idempotency и concurrency

State-changing requests требуют `Idempotency-Key`. Optimistic changes используют `If-Match` или equivalent expected version.

### 27.5 Event stream

WebSocket или SSE передаёт durable events с монотонным cursor. Клиент может reconnect с `after_cursor`.

### 27.6 Error envelope

Категории: validation, authentication, authorization, approval required, version conflict, budget exceeded, unavailable, policy changed, operation unknown, rate limited, internal.

Errors не раскрывают SQL, prompts, secrets или stack traces.

### 27.7 Artifacts API

Staged/resumable upload, hash validation, range download и short-lived handles. SDK не обязан загружать большие файлы целиком в память.

### 27.8 MCP

MCP adapter предоставляет ограниченные memory, run, artifact, agent и workflow operations через тот же application/policy layer.

### 27.9 SDK

Первая очередь:

- Rust SDK;
- TypeScript SDK.

Python SDK является следующей очередью. OpenAPI-generated clients дополняются небольшим ergonomic layer.

### 27.10 Schemas

Публикуются OpenAPI, event schemas, JSON Schemas, MCP schemas и extension protocol schemas. Contract tests проверяют runtime against published schemas.

## 28. Self-hosted product boundary

Первый продукт поддерживает:

```text
Personal
Team
Embedded
```

- Personal: один пользователь, локальный deployment;
- Team: несколько principals и workspaces на self-managed server;
- Embedded: Vestrace как runtime другого продукта через HTTP, MCP и SDK.

Архитектура остаётся SaaS-ready: tenant-aware workspace boundaries, RLS, quotas, audit, storage references и deployment policies. Billing, public signup, region placement и managed fleet не входят в v0.2.

## 29. Reference Agent Packages

### 29.1 Universal Assistant Pack

Coordinator принимает произвольную задачу, выбирает Direct/Guided/Workflow, активирует разрешённые Skills, делегирует, объединяет и проверяет outcome.

### 29.2 Research Pack

Показывает evidence gathering, source comparison, claim verification, independent verifier и report generation.

### 29.3 Workspace Automation Pack

Показывает files/documents, structured extraction, artifact transformations, previews, approvals и scheduled runs.

Coding skills допускаются как отдельные Skills и Docker tools, но полноценный software-engineering package не является release gate v0.2.

## 30. Обязательный vertical slice

Демонстрационная задача:

> Изучи варианты решения задачи, сравни их, подготовь документ и сохрани результат в рабочем пространстве.

Vestrace обязан:

1. создать AgentRun;
2. собрать AgentRuntimeSnapshot;
3. построить и валидировать план;
4. получить memory context;
5. делегировать research subruns;
6. получить evidence через tools;
7. сравнить и проверить результаты;
8. создать документ в Docker sandbox;
9. валидировать artifact и provenance;
10. представить preview;
11. после разрешённого действия экспортировать deliverable;
12. создать memory candidates;
13. продолжить тот же Run после рестарта worker.

## 31. Release acceptance criteria

Harness v0.2 считается готовым, когда подтверждены следующие инварианты:

1. Run переживает рестарт server и worker.
2. Конкурентные workers не продолжают один Run одновременно.
3. Повтор idempotent command не создаёт второй logical operation.
4. Non-idempotent action с unknown outcome не повторяется без reconciliation.
5. Подагент не выходит за delegated capabilities, memory scopes, connections и budget.
6. Опасная операция невозможна без точного связанного approval.
7. External evidence не может повысить permissions или изменить policy.
8. Любой UserDeliverable имеет content hash, source provenance и validation record.
9. Model selection проходит через Policy Engine и объяснимый Router.
10. Budget hard limit нельзя превысить конкурентными reservations.
11. `Succeeded` невозможен без обязательных success criteria.
12. Run может завершиться Partial с полезным и доказуемым outcome.
13. Memory candidates не активируются в обход write policy v0.1.
14. Secrets не попадают в model-visible context, ordinary logs или exported artifacts.
15. HTTP, CLI и MCP проходят через один application/policy layer.
16. Personal и Team deployments используют одинаковые domain contracts.
17. Published schemas проходят contract tests.
18. Reference vertical slice выполняется после полного рестарта runtime.

## 32. Нормативные security invariants

1. Модель не может выдать capability, approval, credential или policy decision.
2. Tool output и external content не интерпретируются как system instructions.
3. AgentProfile, Skill или Package не расширяет system/workspace ceiling.
4. Подагент получает только делегированный context и access grants.
5. Secret material не передаётся модели.
6. Sandbox имеет explicit mounts и network policy.
7. Approval invalidates при изменении normalized operation.
8. Commit-like external actions требуют verification, когда external system это поддерживает.
9. Unknown completion является отдельным состоянием.
10. Audit и execution journal являются append-only.
11. Artifact byte revisions неизменяемы.
12. Hard purge распространяется на derived representations и caches.
13. Worker не принимает authorization decision самостоятельно.
14. Trigger payload не определяет principal, permissions или objective template.
15. Current policy применяется ко всем новым опасным действиям даже внутри старого Run.

## 33. Декомпозиция реализации

Эта спецификация слишком велика для одного implementation plan. Реализация должна быть разбита на отдельные согласованные планы после Foundation и Memory v0.1:

```text
H0. Harness domain foundations
H1. Durable Run and planning runtime
H2. Tool Runtime and sandbox
H3. Policy, approvals and budgets
H4. Context, artifacts and memory consolidation
H5. Model runtime and routing
H6. Delegation and verification
H7. Triggers, conversations and channels
H8. Connectors and credential broker
H9. Extensions and Agent Packages
H10. Public API, SDK and reference vertical slice
H11. Evaluation, replay and release hardening
```

Каждый план должен иметь собственные migrations, contract tests, acceptance tests и branch. Нельзя реализовывать все подсистемы одним неограниченным change set.

## 34. Отношение к существующему roadmap v0.1

Текущие планы v0.1 не отменяются:

1. Foundation;
2. Memory Core;
3. Retrieval and Context;
4. Interfaces and Security;
5. Cognitive Runtime Foundation.

Они создают prerequisite слой. Harness implementation начинается после стабилизации Foundation и необходимых доменных контрактов Memory Core. Отдельные harness seams могут быть добавлены заранее только когда они не расширяют scope текущего плана.

В частности:

- v0.1 `AgentDefinition`, `SkillDefinition` и `WorkflowDefinition` должны эволюционировать совместимо в `AgentProfile`, `SkillDefinition`, `WorkflowDefinition` и `AgentRuntimeSnapshot`;
- v0.1 execution records становятся источником данных для полного `AgentRun` journal;
- v0.1 model registry/router остаётся основой policy-constrained Model Runtime;
- v0.1 MCP и HTTP contracts остаются совместимым подмножеством будущего public API;
- v0.1 memory write policies остаются обязательными для run consolidation.

## 35. Итоговые решения

Утверждены:

- универсальный агент для любых задач;
- Coordinator + контролируемые SubRun;
- изолированный delegated context и typed handoff artifacts;
- динамические versioned plans в deterministic framework;
- единый Tool Runtime и сменные adapters;
- durable current state + append-only journal + checkpoints;
- risk-oriented verification;
- versioned triggers и autonomy levels;
- layered context system;
- Run-first channels;
- centralized Policy Engine;
- policy-constrained Model Router;
- Connector Registry и Credential Broker;
- PostgreSQL metadata + content-addressed Artifact Store;
- modular control/worker/execution topology;
- contract-based isolated extensions;
- AgentProfile + Skills + Workflow + AgentPackage;
- hierarchical reserved budgets;
- separated telemetry, journal, audit и evals;
- versioned HTTP API, durable events, MCP и SDK;
- self-hosted-first Personal, Team и Embedded boundary;
- universal vertical slice с Universal Assistant, Research и Workspace Automation packages.

## 36. Финальный архитектурный принцип

> Vestrace не пытается сделать модель авторитетным управляющим процессом. Модель формирует предложения и содержательные результаты; авторитетное состояние, права, выполнение, доказательства, стоимость, восстановление и последствия принадлежат harness.
