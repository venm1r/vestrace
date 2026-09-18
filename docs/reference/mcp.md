# MCP integration

**Scope:** Current tools and integration boundaries, not certification of a particular external client.

MCP adapts existing application authorities; it is not an independent store. The reviewed server authorizes tool calls and rejects unknown tools.

`search_memories` constructs an ordinary RetrievalRequest. `get_memory` returns id/kind/status, not content. Those tools alone therefore do not provide the complete end-user recall workflow. F101/F102 first need safe content reads; attaching an external client cannot resolve missing output fields.

The CLI provides `mcp` mode. Verify transport, initialization, and credential binding against the selected binary/source. This guide does not present a universal DSH/Cursor configuration as tested interoperability.

## Extension sequence

Start with read-only tools and one qualified client, then explicit user writes, then separately opt-in capture. Recall supplies source-attributed data, not privileged instructions. Model capture is a provenance-bearing observation, not automatically trusted knowledge.

Trusted transport/client setup binds identity, not a freely chosen LLM workspace argument. A memory connection does not govern the other tools in the external runtime. Deleting local memory cannot erase context already delivered to that runtime's logs; disclosure policy must be explicit.

**Sources:** [MCP server](../../crates/vestrace-mcp/src/server.rs), [CLI](../../crates/vestrace-cli/src/main.rs), [integration design](../design/integration-boundaries.md).
