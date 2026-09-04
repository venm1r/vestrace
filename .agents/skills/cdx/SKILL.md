---
name: cdx
description: Codex Sol leads non-trivial software development while a delegated Codex worker implements. Use when Codex Sol should own requirements, architecture, planning, review, and acceptance while implementation remains isolated in a worker subagent.
---

# Cdx

Codex Sol is the lead engineer in the primary agent thread.

A delegated Codex worker is the implementation engineer.

The workflow deliberately separates:

- reasoning from implementation
- architecture from execution
- implementation from acceptance
- builder claims from independent verification

## Ownership

Codex Sol owns:

- requirements
- repository investigation
- architecture
- technical decisions
- planning
- acceptance criteria
- review
- risk assessment
- final approval

The Codex worker owns:

- implementation
- tests
- task-scoped refactoring
- verification during implementation
- fixes requested during review

Do not let the worker make final architectural, scope, or acceptance decisions.

Do not write substantial implementation code in the primary Codex Sol
thread while this workflow is active.

Small non-implementation edits such as updating `PLAN.md` are allowed.

## Agent model

The primary thread is the lead.

Preferred primary configuration:

gpt-5.6-sol / high

Use the built-in `explorer` subagent for broad read-heavy repository
investigation when useful.

Use the built-in `worker` subagent for implementation.

Preferred worker configuration:

gpt-5.6-terra / high

The worker may be given a stronger reasoning level when the task
requires it.

Do not use multiple write-capable workers on the same implementation
unless the work is explicitly partitioned into independent files or
worktrees.

## Subagent threads — always report to the user

Steps 3, 4, and 6 use native Codex subagent threads.

Whenever a delegated agent returns a meaningful result, report that
result to the user in the same turn.

Do not hide:

- review findings
- implementation reports
- test failures
- build failures
- authentication failures
- permission failures
- blocked commands
- unanswered clarifying questions
- agent failures

Do not describe a subagent as finished merely because it was spawned or
accepted the task.

A task is finished only when its agent thread has returned a real
terminal result.

If a worker or reviewer fails, report the failure explicitly before
retrying, redirecting, or changing strategy.

When practical, include the important part of the subagent's own report
close to verbatim.

Do not silently replace a failed task with a new task.

If the user asks for the status of an active subagent:

- inspect the active agent threads
- in Codex CLI, use `/agent` when appropriate
- in environments with a subagent panel, inspect the corresponding
  agent thread
- report the actual state rather than guessing

For a genuine completed review or implementation report, expose the
result before making consequential decisions based on it.

Codex Sol remains responsible for interpreting the result.

## 1. Investigate

Before asking the user questions, inspect the repository.

Read relevant:

- `AGENTS.md`
- applicable nested `AGENTS.md` or `AGENTS.override.md`
- `README`
- architecture documentation
- manifests
- dependency configuration
- build configuration
- existing implementation
- tests
- git status
- current diff

Codex automatically applies applicable `AGENTS.md` instructions.
Treat them as repository constraints.

Use the built-in `explorer` subagent for broad repository search when
that keeps noisy exploration out of the primary thread.

Prefer targeted exploration over indiscriminate repository-wide reads.

Do not ask questions that can be answered from the repository.

Ask the user only when ambiguity materially affects:

- behavior
- architecture
- public API
- data model
- security
- compatibility
- destructive actions
- task scope

For small, local, reversible implementation details, make a reasonable
assumption instead of interrupting the user.

Record material assumptions in `PLAN.md`.

## 2. Plan

For non-trivial tasks create `PLAN.md`.

Keep it concise enough to remain useful as an implementation contract.

Use:

# Goal

# Requirements

# Non-goals

# Constraints

# Acceptance criteria

# Implementation plan

# Verification

Acceptance criteria must be observable and testable.

Requirements should describe externally meaningful behavior rather
than implementation preferences whenever possible.

The implementation plan should identify:

- affected components
- expected interfaces
- important invariants
- migration requirements when applicable
- test strategy
- compatibility constraints

Do not over-design speculative future requirements.

Prefer the smallest architecture that satisfies the current task
without creating obvious structural debt.

## 3. Challenge the plan

For a non-trivial change, run one independent planning review before
implementation.

Spawn a NEW subagent thread.

Prefer the built-in `explorer` agent for this review.

Request:

- gpt-5.6-terra
- high reasoning
- READ-ONLY behavior

Tell it:

"Read PLAN.md and inspect the relevant repository code.

READ ONLY.

Do not modify files.

Critique the plan adversarially.

Look for:

- misunderstood requirements
- unnecessary complexity
- missing edge cases
- architectural conflicts
- compatibility problems
- security problems
- concurrency problems
- data-integrity risks
- incorrect assumptions
- missing tests
- missing failure handling
- simpler solutions

Return only meaningful findings classified as:

BLOCKER
MAJOR
MINOR

For every finding include:

- what is wrong
- why it matters
- concrete repository evidence when available
- the smallest reasonable correction

Finish with:

VERDICT: APPROVE

or:

VERDICT: REVISE"

Codex Sol evaluates every finding independently.

The reviewer is an advisor, not the authority.

Reject speculative or unsupported findings.

Update `PLAN.md` only for valid findings.

Do not perform a fixed number of review rounds.

One independent planning review is normally enough.

Run another planning review only when the first review exposes a
material architectural flaw and the resulting plan changes
substantially.

## 4. Build

Spawn a NEW worker subagent thread.

This becomes the persistent builder thread for the task.

Prefer:

- built-in `worker`
- gpt-5.6-terra
- high reasoning

Tell the worker:

"Implement the approved PLAN.md.

PLAN.md is the implementation contract.

Read applicable AGENTS.md instructions before editing.

Rules:

- inspect repository conventions before editing
- preserve existing user changes
- stay inside task scope
- prefer the smallest coherent implementation
- preserve existing public behavior unless PLAN.md explicitly changes it
- add or update tests where appropriate
- run relevant tests
- run relevant builds
- run relevant linters or static checks
- do not commit
- do not push
- do not deploy
- do not modify secrets
- do not bypass repository protections
- do not weaken tests merely to make them pass
- do not delete failing tests unless PLAN.md explicitly requires their removal
- do not hide verification failures

When finished report:

1. changed files
2. implementation summary
3. tests added or changed
4. verification performed
5. verification results
6. failures or limitations
7. assumptions
8. remaining risks"

Do not start a second implementation worker merely because the first
one encounters a fixable problem.

Preserve the builder thread for Step 6.

## 5. Review

Never trust the worker's completion report by itself.

Codex Sol is the independent quality gate.

After implementation, independently inspect:

- `git status`
- complete relevant diff
- every changed file
- added and modified tests
- relevant existing tests
- verification output
- generated files when relevant

Compare the implementation directly against `PLAN.md`.

Verify:

- all requirements
- every acceptance criterion
- correctness
- edge cases
- error handling
- security
- authorization
- compatibility
- concurrency
- data integrity
- persistence behavior
- migrations
- resource cleanup
- failure behavior
- unnecessary complexity
- unrelated modifications
- test quality
- regression coverage

Do not treat passing tests as proof of correctness.

Do not treat a plausible diff as proof that tests passed.

Distinguish:

- observed facts
- worker claims
- Codex Sol verification
- remaining uncertainty

For each problem found, classify it as:

BLOCKER

The implementation cannot be accepted.

MAJOR

The task may work partially but violates an important requirement,
invariant, compatibility expectation, or acceptance criterion.

MINOR

A real issue exists but does not invalidate the core implementation.

Do not send style-only preferences back to the worker unless they
violate repository conventions or create a concrete maintenance risk.

Codex Sol decides whether the implementation is acceptable.

## 6. Fix

If review finds problems, continue the SAME worker agent thread created
in Step 4.

Do not create a fresh builder.

Send only verified findings.

Tell it:

"Continue the existing implementation.

Fix only these verified review findings:

...

For every finding:

- identify the root cause
- make the smallest correct fix
- preserve unrelated code
- add a regression test when appropriate
- rerun relevant verification
- report the exact verification result

Do not expand scope.

Do not refactor unrelated code."

After the worker returns, Codex Sol independently reviews the new diff
again.

Do not accept the worker's claim that a finding is fixed without
checking the resulting code or verification evidence.

Repeat the review/fix cycle only while real acceptance issues remain.

Do not perform arbitrary fixed review rounds.

## 7. Escalation

Normal worker:

gpt-5.6-terra / high

Use Terra xhigh for genuinely difficult implementation involving:

- concurrency
- distributed systems
- subtle database correctness
- risky migrations
- difficult root-cause debugging
- memory-safety reasoning
- complicated lifetime or ownership behavior
- large interacting refactors
- subtle state machines

Escalate the builder to Codex Sol when:

- Terra repeatedly fails on the same verified issue
- correctness is unusually critical
- the implementation requires substantially stronger cross-system reasoning
- repeated fixes produce regressions
- the root cause remains unresolved after a serious attempt

A model escalation does not transfer architectural authority.

Codex Sol in the primary thread remains the lead and acceptance gate.

Use xhigh only when justified.

Use max or stronger reasoning only for exceptional cases where the
expected correctness benefit clearly justifies the additional cost and
latency.

Do not use maximum reasoning by default.

## 8. Finish

Approve only when the acceptance criteria are satisfied.

Before final approval confirm:

- implementation matches `PLAN.md`
- required tests pass
- relevant builds pass
- relevant static checks pass
- no unexplained changes remain
- no BLOCKER findings remain
- no unresolved MAJOR findings remain unless explicitly accepted as a caveat
- remaining risks are understood

Return briefly:

- what was built
- important problems found during review
- important fixes made
- verification performed
- remaining risks

Final status must be exactly one of:

APPROVED

APPROVED WITH CAVEATS

NOT APPROVED

Use `APPROVED WITH CAVEATS` only when the implementation satisfies the
task but a material non-blocking limitation or external uncertainty
remains.

Use `NOT APPROVED` when any acceptance criterion remains unsatisfied or
a correctness-critical issue is unresolved.
