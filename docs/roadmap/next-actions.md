# Next actions and acceptance order

**Status:** Proposed sequence. No implementation checkbox is marked complete here.

Use one implementation package alongside an independent documentation or measurement task.

## A. Pin the current checkout

- [ ] Compare HEAD with the supplied `3e05dfbd` snapshot and record any newer baseline.
- [ ] Read the latest P04 evidence, including Task 14E acceptance and its limitations.
- [ ] Capture dirty/untracked state and exact writable/protected paths.
- [ ] Decide MW milestone placement without changing frozen scope implicitly.

```bash
git rev-parse HEAD
git status --porcelain=v1 -z --untracked-files=all
git diff --name-status 3e05dfbdce063aa44a3a9e5a7a84c274597e8188 HEAD
```

Do not publish dirty file contents containing credentials or user data. Check the migration
catalog: `0196_retired_credential_erasure.sql` already occupies the number used by candidate
MW-M01. MW-00 must reconcile candidate names and every dependent reference before SQL work.

## B. Finish the accepted foundation task

Use the existing detailed P04 continuation, not a rewritten approximation in this roadmap.
Verify roles, outcome identities, guards, and crash recovery without another dispatch.
Do not repeat Task 14E just because earlier guidance called it proposed. Do not mix MW writer/
migration work into protected P04 scope without an accepted amendment.

## C. Prepare the first MW vertical slice

Complete [MW-00](../implementation/memory-workspace/plans/00-preflight.md). Start MW-01 with
real detail/history query ports. Before editor acceptance, finish the MW-02 atomic writer.
Do not solve missing APIs by giving Console database access. Choose a small synthetic
read → edit → reload example before implementing every importer format.

## D. Measure without disturbing implementation scope

Record cold-build and test-cycle cost, RAM/disk, and environment blockers. Label a small
current/old/corrected/no-evidence corpus with results still NOT_RUN. Prepare a safe token/
Console workflow without compiling credentials into the browser.

## Handoff

Return exact changed files, linked requirements, failing/passing observations, final SHA,
environment, checks not run, review verdict, and next dependency. A “done” label closes
neither a milestone nor a specification. Detailed task steps remain in the eight MW plans.

[Roadmap](README.md) · [Development workflow](../development/agent-workflow.md)
