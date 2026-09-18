# Troubleshooting without destructive shortcuts

| Symptom | Check first | Avoid |
| --- | --- | --- |
| API 401 | Bearer token and token-store resolution | Changing workspace headers to impersonate identity |
| Vite loads but API returns 401 | Vite versus nginx authentication | Compiling an admin token into JavaScript |
| Bootstrap/root failure | Mounted layout, declared identity, writable material root, no overlap | Treating an empty volume as configured prerequisites |
| Incompatible schema | Build's migrations, success records, checksums | Editing applied SQL or checksums manually |
| Memory saved, search empty | Identity/policy, lifecycle, channels, generation readiness | Equating persisted with indexed |
| ContextPack lacks text | Current HTTP DTO | Treating section_count as a delivered prompt |
| Revision conflict | Accessible current revision and concurrent writer | Blind overwrite using a newly fetched version |
| Growing outbox | Handler, due/backoff/dead-letter, provider dependencies | Deleting pending rows for green health |
| Idle worker | Configured workspace and work state | Calling idle corruption or whole-Run success |
| ResultPrepared after restart | Durable markers and binding/publication progress | Repeating provider calls or forcing Succeeded |
| UI returns 501 | Specific handler implementation | Simulating success in the frontend |

Separate a diagnostic hypothesis from an established cause. Pin source/runtime and safe observations when reproducing. An untested environment is not automatically unsuitable, but it is not qualified either.

Reports should state expected behavior, actual status/message, minimal reproduction, and possible impact. Exclude credentials, authentication headers, and user content. Preserve identity and uncertainty for dangerous effects, not only stack traces.

**Sources:** [HTTP](../reference/http.md), [Console](../guides/console.md), [runbook](runbook.md), [current status](../status.md).
