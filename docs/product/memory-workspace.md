# Memory Workspace

**Status:** Proposed product extension. This page defines user value; the [implementation package](../implementation/memory-workspace/README.md) is the single detailed specification.

## One coherent workflow

Import a document → find and read the relevant knowledge → inspect its source and revision → correct it → request useful context → synchronize an updated source without losing the correction → export an authorized selection.

The Console, CLI, and background handlers remain clients of one canonical memory model and one governed write boundary. The source document and effective memory can legitimately differ after a human correction; the interface must show that distinction.

## Scope

| Capability | User-facing result | Primary package |
| --- | --- | --- |
| Memory and context reads | Content, exact revisions, history, browse, and bounded rendered context | MW-01 |
| Corrections and restore | Version-checked changes, preserved history, and durable replay receipts | MW-02 |
| Console | Library, detail, editor, conflict handling, and honest availability states | MW-03 |
| Source import | Bounded formats, exact-byte preview, explicit Apply, and durable progress | MW-04 |
| Synchronization | Base/incoming/manual conflicts, explicit resolution, Missing and cancellation | MW-05 |
| Portability | Authorized JSON/Markdown export and foreign-to-local import mapping | MW-06 |
| Acceptance | Upgrade, restricted-role, fault, and end-to-end checks | MW-07 |

MW-00 precedes implementation and reconciles the current source tree, protected scope, and shared authorities.

## Deliberate limits

The first importer accepts small Markdown/plain-text documents and the defined JSON format. It does not need an LLM, crawler, automatic watcher, PDF parser, OCR, or semantic auto-merger. Detailed byte and batch limits are in the package; they are product limits, not benchmark results.

A familiar interface must not hide incompatible revisions, missing content, unavailable indexing, or a denied historical read. A successful import is not automatically a searchable generation. A byte-bounded response is not a qualified hard-token guarantee.

## Release relationship

The extension remains Proposed and its release placement is unassigned unless the owner explicitly amends a program. It neither becomes P13–P20 nor removes obligations from the frozen P01–P12 v1.0 program.

See [design decisions](../implementation/memory-workspace/01-design.md), [API](../implementation/memory-workspace/03-api.md), and [program mapping](../roadmap/program-mapping.md).
