# Console

## Container and development modes differ

The containerized Console uses nginx. Its proxy adds Authorization from runtime environment rather than embedding the token in JavaScript. Access to that port effectively grants the configured token's capabilities. The default bind is loopback; exposing it requires a deliberate authentication boundary, not changing `127.0.0.1` to `0.0.0.0` alone.

The reviewed Vite proxy handles `/api/v1`, `/api/ag-ui`, and health but does not inject Bearer authentication like nginx. VITE_VESTRACE_WORKSPACE_ID and VITE_VESTRACE_PRINCIPAL_ID are not credentials. Never put a secret VITE_* token in the bundle as a workaround. Verified development-proxy authentication is part of F004.

## Frontend checks

```bash
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
```

These do not establish backend availability or successful authentication. Use package.json for the script inventory. The original snapshot has no general `npm test`; MW-03 proposes memory and browser scripts rather than assuming they exist.

## Pages versus complete workflows

Console contains Runs, Agents, Models, Connections, and other routes. Individual actions may remain disabled, partial, or return 501. Preserve those distinctions instead of simulating success.

MemoryConsole.tsx exists as a presentation component, but the reviewed router lacks a complete memory-library/editor route. A component is not a usable feature by itself.

## Selected extension

The target is library → detail → history → correction → reread after reload. Console calls public APIs without direct SQL or trusted actor selection. Source text and editorial revisions remain separate; synchronization conflicts cannot be hidden behind a success notification.

Reuse existing design tokens/components rather than create another frontend or graph editor. See the [MW Console design](../implementation/memory-workspace/04-console.md).

**Sources:** [Vite](../../apps/console/vite.config.ts), [nginx](../../apps/console/nginx.conf.template), [router](../../apps/console/src/main.tsx), [package scripts](../../apps/console/package.json).
