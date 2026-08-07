# Security & Tenant Isolation Architecture

Vestrace is built with defense-in-depth security to guarantee multi-tenant isolation, data privacy, and fine-grained capability authorization.

## 1. Multi-Tenant Isolation (Row-Level Security)

All multi-tenant database tables enforce PostgreSQL Row-Level Security (RLS). 

### How RLS Works in Vestrace
1. Incoming requests authenticate and establish a `RequestContext` containing the verified `WorkspaceId` and `PrincipalId`.
2. Every database operation executes within a scoped transaction via `PgTransactionManager`.
3. The transaction manager executes:
   ```sql
   SELECT set_config('vestrace.workspace_id', '<workspace-uuid>', true);
   SELECT set_config('vestrace.principal_id', '<principal-uuid>', true);
   ```
   These are semantically equivalent to `SET LOCAL` but use the `set_config` function for parameterized binding.
4. PostgreSQL RLS policies evaluate `vestrace_current_workspace_id()` and reject any read/write attempt crossing workspace boundaries.

## 2. Authorization & Capability Model (RBAC)

Access rights are defined as granular `Capability` tokens parsed from string identifiers:
- `memory.read`: Read active memory records and revisions.
- `memory.write`: Create candidates, revise memories, or change memory status.
- `memory.purge`: Execute administrative hard purge of memories and derivatives.
- `event.read` / `event.write`: Access append-only event stream.
- `context.retrieve`: Execute candidate retrieval and assemble context packs.

## 3. Sensitivity Classification & Redaction

Data items carry a `Sensitivity` level:
- `Public` (0)
- `Internal` (1)
- `Confidential` (2)
- `Restricted` (3)

Prior to external provider transmission (e.g., sending context to an LLM), text is processed through active `redaction_rules` to prevent secret leakage.
