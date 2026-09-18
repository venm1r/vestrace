# Product scenarios

**Type:** User goals and target workflows. Use [implementation status](../status.md) to distinguish available surfaces from proposals.

## A project across sessions

A new agent session should recover the project's decisions, constraints, and unfinished work without treating its current prompt as the only history. References must identify the memory revisions that support the result. A second client should see an accepted correction rather than an unrelated copy of the old context.

Acceptance is not “a search returned text.” It requires the right permitted evidence, the right temporal meaning, and a usable context representation. An inaccessible or unsupported path must be reported explicitly.

## Correct knowledge without erasing history

A user reads a specific revision, supplies a reason, and writes a correction under a version precondition. Another writer's intervening change produces a conflict, not silent overwrite. Restoring old text creates another revision rather than erasing subsequent history. Classification changes require their own governance path.

## Keep source updates and human edits separate

Import a bounded documentation folder, inspect a preview, and apply exactly that snapshot. Rescan the same sources without duplicating unchanged knowledge. If upstream and a human editor both changed a record, preserve the base, incoming source, and current memory until an explicit resolution.

Missing files are observations, not deletion instructions. Renames require stable identity or a confirmed mapping. These rules belong to the [Memory Workspace proposal](../implementation/memory-workspace/README.md).

## Explain changes in context

An operator should be able to inspect why one context differs from another: changed source revisions, temporal selection, permissions, generation, or budget. Distinguish the context Vestrace issued, a client's report of using it, and an independently observed model request. None is interchangeable with the others.

## Transfer knowledge without transferring privileges

An authorized export contains a bounded selection and its permitted history/provenance. Import assigns new local identities and treats foreign actors, time, and confidence as source annotations. It does not import capabilities, credentials, qualification, or the right to choose local classification. Knowledge export is not installation recovery.

## Operate through failures

A worker restart should expose durable progress and unresolved outcomes, not fabricate success or repeat an external effect blindly. Operators must be able to distinguish preparation, publication, index readiness, and a completed Run. A backup requires an actual restore experiment before relying on it.

The first release should qualify one coherent scenario before widening the promise. See [milestones](../roadmap/milestones.md), [acceptance](../implementation/memory-workspace/09-acceptance.md), and [release rules](../development/release.md).
