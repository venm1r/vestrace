# Vestrace Data & Temporal Model v0.2

**Статус:** normative data/temporal specification  
**Дата:** 2026-08-10  
**Базовые документы:** Architecture Contract, Domain Model, Invariants Catalog.

## 1. Цель

Этот документ определяет, как Vestrace представляет время, версии, validity, historical state, provenance и temporal queries без смешения факта события, времени его записи и периода применимости знания.

Ключевой принцип:

> **Time of occurrence, time of recording, time of validity and object revision time are separate axes.**

---

# 2. Temporal axes

## 2.1 `occurred_at`

Время, когда source утверждает/показывает, что событие произошло во внешнем или предметном мире.

Свойства:

- MAY быть unknown;
- MAY быть неточным;
- MAY прийти позднее `recorded_at`;
- не определяет authoritative ordering внутри Vestrace без дополнительного contract.

## 2.2 `recorded_at`

Время authoritative фиксации факта Vestrace.

Для append-only domain fact `recorded_at` MUST быть известно.

`recorded_at` используется для audit/history ordering там, где source occurrence time не надёжен.

## 2.3 `created_at`

Время создания конкретной domain entity/revision в Vestrace.

Для immutable fact часто совпадает с `recorded_at`, но семантически остаётся отдельным полем, если lifecycle требует различия.

## 2.4 `valid_from` / `valid_until`

Интервал предметной применимости знания.

Пример:

```text
recorded_at: 2026-08-10
valid_from:  2026-09-01
```

означает: Vestrace уже знает факт, который станет применим позже.

## 2.5 Processing timestamps

Operational timestamps вроде `started_at`, `leased_at`, `finished_at`, `retrieved_at`, `verified_at` описывают execution/processing и MUST NOT подменять semantic validity.

---

# 3. Unknown and uncertain time

## 3.1 Unknown is null/explicit uncertainty

Если время неизвестно, Vestrace MUST NOT создавать фиктивный timestamp вроде epoch/`now()` для заполнения поля.

## 3.2 Time precision

Источник MAY иметь precision:

- exact instant;
- minute/hour/day;
- date-only;
- interval;
- unknown.

Если precision materially влияет на reconciliation, она SHOULD сохраняться как metadata.

## 3.3 Time confidence

Temporal confidence MAY быть отдельным evidence attribute и не должна автоматически смешиваться с semantic claim confidence.

---

# 4. Versioning model

## 4.1 Identity vs revision

Stable identity:

```text
MemoryId / AgentId / WorkflowId / PolicyId
```

Exact historical content:

```text
MemoryRevisionId / AgentRevisionId / WorkflowRevisionId / PolicyVersionId
```

Ссылка, используемая для reproducibility, SHOULD указывать exact revision.

## 4.2 State revision

Mutable lifecycle identities MAY иметь monotonic `state_revision` для optimistic concurrency независимо от content revision numbering.

Пример:

- Memory content revision #5 unchanged;
- lifecycle `Active → Superseded` меняет aggregate state revision.

## 4.3 Expected revision

Concurrent mutation MUST проверять expected state/revision.

```text
read v12
   ↓
prepare mutation(expected=v12)
   ↓
current=v13
   ↓
STALE / CONFLICT
```

Silent overwrite forbidden.

---

# 5. Append-only history

## 5.1 Facts

Events, immutable revisions, receipts, audit entries, assessments и verification results после commit не изменяются.

## 5.2 Corrections

Correction формируется как новый fact/revision с relation:

- `corrects`;
- `supersedes`;
- `compensates`;
- `invalidates`;
- `reconciles`.

## 5.3 Tombstones

Удаление content MAY сохранять content-free tombstone с typed identity/hash/time/reason, если это требуется provenance/audit policy.

Tombstone MUST NOT содержать запрещённый удалённый payload.

---

# 6. Knowledge validity

## 6.1 Current applicability

Knowledge считается candidate для current retrieval только если одновременно:

- lifecycle допускает current use;
- current time находится внутри validity interval либо interval open/unknown согласно policy;
- conflict state не запрещает unqualified current use;
- evidence/provenance остаётся admissible;
- authorization/governance допускает disclosure.

## 6.2 Supersession

Supersession SHOULD содержать:

```text
superseded_ref
superseding_ref
reason
recorded_at
semantic_effective_at?
```

`semantic_effective_at` MAY отличаться от времени фиксации supersession.

## 6.3 Expiry

Expiry означает прекращение current applicability по времени/policy, но history сохраняется.

---

# 7. Claim temporal semantics

## 7.1 Claim proposition vs assessment

Claim semantic content и оценка его поддержки разделены.

Одно утверждение может иметь последовательность assessments во времени без изменения proposition.

## 7.2 Claim validity

Claim MAY иметь own validity interval, отличающийся от времени supporting evidence.

## 7.3 Contest timing

Claim становится `Contested` с момента authoritative фиксации admissible contradiction, если policy не устанавливает иной deterministic threshold.

Исторический `as_of` до фиксации contradiction MAY показывать claim как supported, даже если сегодня известно больше.

Это distinction между:

- **what was true/valid then**;
- **what Vestrace knew then**;
- **what Vestrace knows now about then**.

Полная bitemporal interpretation раскрывается query policy, но система MUST сохранять данные, необходимые для этого различия.

---

# 8. Temporal perspectives

## 8.1 `Current`

Возвращает текущую effective cognitive state с current lifecycle/conflict/policy semantics.

## 8.2 `AsOf`

`AsOf(t)` должен явно указывать semantic mode:

### `KNOWN_AS_OF`

Что было authoritative известно Vestrace к `recorded_at <= t`.

### `VALID_AS_OF`

Что согласно доступной query state применимо к предметному времени `t`.

### `RECONSTRUCTED_AS_OF`

Современная реконструкция состояния предметного мира на время `t`, которая MAY использовать evidence, полученное после `t`.

Эти режимы MUST NOT silently conflated.

## 8.3 `Timeline`

Возвращает ordered changes/claims/evidence с указанием оси ordering.

## 8.4 `AllHistory`

Может включать superseded/rejected/expired/contested history, но MUST маркировать lifecycle и current applicability.

---

# 9. Ordering and causality

## 9.1 Aggregate sequence

Внутри authoritative stream/aggregate используется monotonic sequence/version.

## 9.2 Correlation and causation

Где возможно, facts сохраняют:

- correlation id;
- causation id;
- parent execution/operation reference.

## 9.3 Clock order is not causal order

Если два independent events имеют wall-clock timestamps, Vestrace MUST NOT автоматически утверждать causality только из сравнения timestamps.

## 9.4 Concurrent facts

Concurrent/independent facts MAY оставаться partially ordered.

Reconciliation должен работать с отсутствием полного порядка, а не искусственно создавать его.

---

# 10. Late-arriving evidence

## 10.1 Ingestion after occurrence

Событие, произошедшее раньше, MAY быть записано позже.

```text
occurred_at = T1
recorded_at = T3
```

## 10.2 Effect on current state

Late evidence MAY вызвать:

- claim re-assessment;
- conflict;
- historical correction;
- derived projection invalidation;

но MUST NOT переписывать факт того, что до T3 система этого evidence не знала.

## 10.3 Historical reconstruction

Reconstructed historical query MAY учитывать late evidence, но MUST указывать соответствующий temporal mode.

---

# 11. Retractions and corrections

Источник MAY retract/correct ранее предоставленное evidence.

Vestrace фиксирует новую evidence event/relation, а не удаляет исходную историю по умолчанию.

Claim assessment/reconciliation затем пересчитывается отдельно.

---

# 12. Temporal conflicts

Типичные temporal conflicts:

- overlapping mutually exclusive validity intervals;
- contradictory values for same semantic key/time;
- event order inconsistent with declared causal chain;
- source correction with backdated validity;
- stale revision mutation.

Temporal conflict является first-class conflict/finding согласно domain ownership.

---

# 13. Retrieval time rules

## 13.1 Candidate filtering

Retrieval MUST применять requested temporal perspective до финального current-truth assembly.

## 13.2 Ranking

Recency является ranking signal, а не authority rule.

Более новый item не получает автоматическую semantic победу над более авторитетным/доказанным item.

## 13.3 ContextPack temporal metadata

ContextPack SHOULD сохранять:

- temporal perspective;
- effective query time;
- included revision validity;
- relevant conflict/supersession markers;
- generation/state refs.

---

# 14. Execution temporal semantics

Execution domain использует отдельные timestamps:

- issued/created;
- started;
- waiting/resumed;
- dispatched;
- acknowledged;
- completed;
- recorded.

External effect outcome MAY оставаться unknown после dispatch даже если request timestamp известен.

---

# 15. Repair and temporal preconditions

RepairPlan содержит input state/version and expiry.

Elapsed time или state mutation MAY сделать plan stale даже без изменения finding fingerprint.

Перед execution preconditions MUST быть rechecked.

---

# 16. Sharing temporal semantics

MemoryShareGrant/MemoryMount имеют собственные validity/lifecycle intervals.

Access разрешён только если:

- grant active and valid;
- exact revision accepted;
- mount active/not stale;
- target/source policies current enough according to policy;
- source content still available.

Historical fact использования memory через ранее valid grant сохраняется после revoke.

---

# 17. Policy temporal semantics

Policy decisions всегда связываются с exact policy version.

Новая policy version действует prospectively, если отдельная migration/revalidation policy не требует пересмотра существующего состояния.

Историческая операция оценивается относительно policy, применённой при decision, плюс MAY получить современный compliance finding без переписывания истории.

---

# 18. Data lifecycle time

Retention clock MUST иметь explicit trigger:

- created;
- last used;
- execution closed;
- workspace closed;
- policy event;
- explicit date.

Expiry и physical disposal могут происходить в разное время.

DataHold временно блокирует disposal, но не изменяет original retention facts.

---

# 19. Recovery temporal semantics

Recovery SHOULD сохранять:

- source recovery point;
- snapshot position;
- replay range;
- rebuild generation;
- revalidation time.

После restore current wall clock не используется как замена lost event ordering.

---

# 20. Generation counters

Derived systems MAY использовать monotonic generations:

- memory generation;
- policy generation;
- share generation;
- retrieval/index generation;
- classification generation.

Generation — invalidation aid, не semantic time и не substitute для revision identity.

---

# 21. Normative mappings

Основные requirements:

- `TMP-001..TMP-010`;
- `MEM-007..MEM-019`;
- `MUT-001..MUT-008`;
- `RET-002`, `RET-012`, `RET-013`;
- `HLT-009`;
- `IDW-008`, `IDW-013`;
- `GOV-013`, `GOV-025`.

---

# 22. Forbidden shortcuts

```text
occurred_at == recorded_at
recorded_at == valid_from
newest timestamp == truth
newest timestamp == causal successor
current projection == historical state
expiry == deletion
revoke == erase history
state generation == semantic version
lease timestamp == authority
```

---

# 23. Completion criteria

Data & Temporal Model считается завершённым, когда:

1. Retrieval specification использует explicit temporal perspectives;
2. Domain schemas отличают occurrence/recording/validity/revision time;
3. reconciliation не зависит от universal latest-wins rule;
4. policy/share/retention lifecycles используют exact validity semantics;
5. qualification suite имеет late-arriving evidence, concurrent revision и `as_of` golden scenarios.
