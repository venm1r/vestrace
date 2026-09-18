# Background memory consolidation

**Status:** Proposed functional design for F206.

## First bounded artifact

Begin with an updatable digest of project decisions, constraints, and open questions—not an agent that decides what the system should believe. The user chooses the purpose and permitted source scope.

Inputs are exact permitted revisions plus method/model/policy versions. Output is a derived projection with input references, processing watermark, and freshness state. Requested/building/ready/stale/failed describe that projection or an existing Run view, not another task runtime.

Generating a digest does not change canonical facts. Publishing a canonical memory from it is a separate proposal/mutation with authority and provenance. Usage frequency and model confidence do not automatically raise trust.

## Execution and access

Schedule through existing execution/outbox. Recheck sources and destination before disclosure. Retries use immutable input identity. A changing source yields either a valid snapshot result marked stale or cancellation under the accepted contract; silent generation mixing is prohibited.

A projection cannot disclose more than its sources permit. Lost access invalidates current serving decisions. Lawful historical retention must agree with source/material erasure blockers. Audit is not an indefinite plaintext digest copy.

## Acceptance and dependencies

The digest should help solve a control task with less context without losing decisions or negations. Later correction invalidates freshness. Process recovery must not create two independently accepted results. Recomputing without new information must not increase trusted knowledge.

Start after F201/F202/F204/F008 and an explicit scope decision. Declare optional LLM use and measure cost/latency. Do not build hidden permanent sleep agents, another policy engine, or automatic belief promotion.

**Sources:** [architecture](../specs/en/vestrace-architecture-contract-v0.2.md), [outbox](../../crates/vestrace-application/src/outbox.rs), [materials](../../crates/vestrace-application/src/material/commands.rs).
