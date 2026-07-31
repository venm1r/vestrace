# Vestrace State Engine — boundary amendment

**Статус:** предложено для письменного утверждения  
**Дата:** 2026-07-31  
**Репозиторий:** `venm1r/vestrace`  
**Базовая спецификация:** `docs/superpowers/specs/2026-07-31-vestrace-harness-v0.2-design.md`  
**Основание:** `docs/superpowers/specs/2026-07-31-vestrace-state-engine-reconciliation-design.md`

> Этот документ является documentation-only amendment. Он не разрешает создавать миграции, Rust-код, новый runtime-процесс, отдельную базу данных, implementation branch или публичный интерфейс.

## 1. Назначение

Документ закрепляет единое значение термина `Vestrace State Engine` и распределяет durable-state ответственность между уже утверждёнными Horizons Vestrace Harness v0.2.

Поправка устраняет риск появления параллельной архитектуры после обсуждения специализированного State Engine. Она не добавляет новый продуктовый слой и не меняет roadmap версий.

## 2. Нормативное определение

> **Vestrace State Engine — внутреннее название durable execution-state boundary, образованной существующими H1–H10 application ports, авторитетными PostgreSQL-агрегатами, журналами, checkpoints, policies, artifacts, memory и производными projections.**

State Engine описывает согласованную ответственность существующих компонентов. Он не является самостоятельной исполняемой сущностью.

## 3. Чем State Engine не является

Термин `Vestrace State Engine` не обозначает:

- отдельный продукт;
- отдельный сетевой сервис;
- отдельный binary или обязательную process role;
- отдельный crate или Rust workspace;
- отдельную базу данных;
- второй event store;
- второй orchestration runtime;
- новый public API prefix;
- самостоятельный SQL API;
- параллельную модель исполнения `Run → Task → Step → Action → Attempt`;
- новый Horizon `H12 State Engine`;
- замену Memory Plane, Harness или application ports.

Использование термина в документации не создаёт право собственности на доменные сущности, уже принадлежащие конкретному Horizon.

## 4. Авторитетные владельцы

| Область | Нормативный владелец | Авторитетные контракты |
|---|---|---|
| Run lifecycle | H1 | `AgentRun`, `RunStatus`, `RunVersion`, transitions |
| Execution steps | H1 | `RunStep`, step lifecycle, scheduling attempt |
| Run journal | H1 | `RunEvent`, sequence, append-only commit |
| Checkpoint и logical replay | H1 | `RunCheckpoint`, resume cursor, side-effect-free replay |
| Worker coordination | H1 | leases, work items, restart-safe acquisition |
| Authorization | H2 | policy evaluation, authorization ticket, capability ceiling |
| Approvals | H2 | approval lifecycle и operation binding |
| Budgets и quotas | H2 | reservations, allocations, accounting journal |
| Model execution | H3 | model invocation, provider attempt, routing reference |
| Tool execution | H4 | tool invocation, sandbox execution, idempotency, reconciliation |
| Planning и delegation | H5 | immutable plan revisions, `RunStep` assignment, SubRun |
| Context | H6 | `ContextSnapshot`, exact references, governed capture |
| Artifacts и evidence | H6 | `Artifact`, immutable revision, provenance, mount, retention, purge |
| Memory consolidation | H6 поверх v0.1 | memory candidates через существующие memory application services |
| Interactions и continuations | H7 | conversations, human input/approval continuation, public events |
| Triggers | H7 | trigger evaluation и Run proposal boundary |
| Connections и credentials | H8 | connection revisions, secret references, credential leases |
| Packages и extensions | H9/H9A | package/component authority и A2A interoperability |
| Audit, evaluation и projections | H10 | security audit, telemetry, verification, operational read models |
| Public product adapters | H11/H11A/H11B | HTTP, MCP, SDK, SSE, AG-UI и web application |

Ни одна производная проекция, adapter или worker не становится владельцем авторитетного состояния только потому, что отображает или обрабатывает его.

## 5. Авторитетная execution model

Vestrace остаётся Run-first системой:

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

`AgentRun` является корнем авторитетного исполнения. `ExecutionPlanRevision` задаёт неизменяемый план. `RunStep` связывает план с фактическим выполнением. Конкретные операции и их попытки принадлежат типизированному runtime, который их исполняет.

## 6. Запрет параллельных generic aggregates

Нельзя вводить универсальные mutable aggregates `Task`, `Action` или `Attempt`, если их семантика уже выражена существующими контрактами.

Запрещены следующие дублирования:

```text
Generic Task     вместо AgentRun / ExecutionPlanRevision / RunStep
Generic Action   вместо ModelExecution / ToolInvocation / SubRun / HumanRequest
Generic Attempt  вместо typed provider, tool, remote-agent или verification attempt
Generic Graph    вместо immutable ExecutionPlanRevision dependencies
Generic Status   вместо статусов owning Horizon
```

Новый общий агрегат допустим только после отдельного design review, который доказывает одновременно:

1. существующие типизированные агрегаты не могут выразить требуемую семантику;
2. новый агрегат не создаёт вторую authority boundary;
3. правила lifecycle, ownership, concurrency, audit и recovery определены полностью;
4. migration path не переписывает append-only history;
5. публичные adapters не получают новый обходной путь изменения состояния.

## 7. SQL boundary

PostgreSQL является authoritative persistence backend, но SQL не является domain, application или agent-facing интерфейсом.

Нормативный путь записи:

```text
trusted adapter or worker
→ application command
→ capability and policy evaluation
→ owning application port
→ scoped PostgreSQL transaction
→ authoritative aggregate mutation
→ canonical event/audit fact
→ follow-up work or projection intent
```

Запрещено:

- выдавать агентам произвольный write SQL;
- предоставлять public endpoint для произвольного SQL;
- изменять Run, memory, Artifact, approval, policy или credential rows в обход application ports;
- обновлять материализованную проекцию как способ изменить authoritative state;
- считать PostgreSQL role или connection string достаточной доменной authority;
- использовать read-only analytics view как источник разрешения на действие.

Ограниченные read-only operational views допустимы для administrative и analytical adapters при выполнении всех условий:

- forced RLS и scoped transaction context;
- положительная capability/policy decision;
- отсутствие секретов, backend paths и неразрешённых payload;
- view явно считается производной;
- результат view не используется для изменения authoritative state без повторной проверки через application command.

## 8. Event и projection boundary

H1 `RunEvent` является каноническим журналом logical Run mutations. Остальные Horizons владеют своими обязательными audit/domain facts.

Logging и capture presets не могут:

- удалять обязательное каноническое событие;
- менять семантику event kind;
- отключать обязательный security audit;
- превращать telemetry в источник истины;
- сохранять hidden chain-of-thought или secret material.

Проекции могут быть синхронными или асинхронными в зависимости от критичности, но их потеря или lag не изменяют уже зафиксированную каноническую историю.

## 9. Recovery boundary

State Engine не вводит отдельный recovery runtime. Восстановление распределено по owning contracts:

- H1 — resume, checkpoint и logical replay;
- H3 — reconciliation model/provider execution, если применимо;
- H4 — tool side-effect classification, idempotency и reconciliation;
- H5 — SubRun/remote execution reconciliation;
- H6 — Artifact integrity, quarantine и provenance;
- H8 — credential lease recovery без раскрытия secret material;
- H10 — safe replay, verification и diagnostic evidence.

`Unknown` означает отсутствие подтверждённого исхода. Оно никогда не означает разрешение автоматически повторить внешнее действие.

## 10. Memory boundary

Memory Plane остаётся самостоятельным. Harness получает и изменяет память только через существующие v0.1 application services.

State Engine:

- не изменяет memory tables напрямую;
- не создаёт второй memory journal;
- не активирует candidate memory в обход write policy;
- не трактует Artifact mount как memory-sharing grant;
- не вводит cross-workspace memory sharing без отдельного approved design.

## 11. Artifact и reference boundary

Hash, UUID, URI, alias, storage key, revision reference или mount handle не являются permission.

Право чтения, передачи, экспорта или mount определяется текущими H2/H6 policies и workspace boundary. Physical storage paths, bucket keys, presigned URLs и host paths не должны становиться model-visible domain references.

## 12. Public interface boundary

HTTP, MCP, CLI, SDK, SSE, AG-UI и web console являются adapters над общими application services.

Они не могут:

- вводить собственные Run statuses;
- выполнять mutation без owning application port;
- предоставлять arbitrary SQL;
- выдавать storage credentials;
- локально пересчитывать policy, approval или budget authority;
- автоматически повторять ambiguous external operation;
- активировать imported Run/export bundle;
- создавать новое authoritative состояние только на основании UI state.

## 13. Модульный монолит и process roles

State Engine boundary совместима с modular-monolith архитектурой и несколькими process roles.

Разделение `server`, `worker`, `scheduler`, `sandbox-manager` и других ролей является operational deployment choice. Оно не делит доменную authority и не создаёт отдельные версии состояния.

Все роли используют одни application contracts и одну authoritative PostgreSQL model.

## 14. Версионирование и изменения

Изменение State Engine boundary требует documentation review, если оно:

- переносит authority между Horizons;
- добавляет новый authoritative aggregate;
- меняет Run или step lifecycle;
- создаёт новый event journal;
- вводит новый public mutation path;
- позволяет direct SQL;
- меняет recovery/replay semantics;
- добавляет cross-workspace data sharing;
- изменяет secret или encryption authority.

Новый implementation plan не может сам изменить эту boundary. Сначала требуется отдельный design/ADR и письменное утверждение.

## 15. Documentation-only boundary

Эта поправка не разрешает:

- создавать implementation branch;
- менять Rust source;
- добавлять crate или binary;
- создавать SQL migrations;
- менять dependencies;
- запускать services;
- изменять CI;
- реализовывать State Engine как отдельный компонент;
- начинать последующие amendments без их собственного review gate.

## 16. Итоговый инвариант

> **Vestrace State Engine не расположен рядом с Harness и не управляет им. Это согласованное имя для durable-state ответственности самого Harness, распределённой между существующими owning Horizons и доступной только через их application ports.**
