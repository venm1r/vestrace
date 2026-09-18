# External-agent integration boundaries

**Status:** Proposed functional design for F203.

## Integration modes

**Memory client:** The external runtime owns its session and calls permitted memory tools. Vestrace governs only its own data/operations. An MCP connection does not make the other runtime's tools governed by Vestrace.

**Governed action:** One external action runs entirely through Vestrace's Run/effect authority under a separate contract. Two runtimes must not independently retry the same action. The first memory plugin does not automatically include this mode.

## First client

Choose DSH or another actual client at an identified version. Start read-only, add explicit user writes, then separately opt-in source-attributed capture. A third-party plugin catalog does not establish compatibility.

Verify transport, authentication, schemas, error taxonomy, reconnect, cancellation, and client privacy/storage behavior. Publish configuration examples only after reproducible interoperability, with safe placeholders instead of working credentials.

## Context and writes

Source context is data, not privileged instruction. The client must preserve that authority level. The application chooses required-context versus optional-memory behavior: missing required knowledge blocks the task; optional memory allows continuation with an explicit limitation. A network error does not choose that policy for the plugin.

Capture records client/source identity, content type, and immutable provenance. Replaying a completed turn must not duplicate memory. Model text is not user confirmation. Private/project/team scope cannot be a freely chosen LLM argument.

## Acceptance

Two sessions under the same permitted identity see an accepted correction; another identity cannot read it. Client restart does not repeat a mutation. Unavailability follows documented application policy. Explain that already delivered bytes may remain in external logs and cannot be recalled by local deletion.

**Sources:** [MCP](../../crates/vestrace-mcp/src/server.rs), [authentication](../../crates/vestrace-http/src/auth.rs), [Architecture Contract](../specs/en/vestrace-architecture-contract-v0.2.md).
