CREATE FUNCTION vestrace_current_workspace_id()
RETURNS UUID
LANGUAGE sql
STABLE
AS $$ SELECT nullif(current_setting('vestrace.workspace_id', true), '')::uuid $$;

CREATE FUNCTION vestrace_current_principal_id()
RETURNS UUID
LANGUAGE sql
STABLE
AS $$ SELECT nullif(current_setting('vestrace.principal_id', true), '')::uuid $$;

ALTER TABLE workspaces ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspaces FORCE ROW LEVEL SECURITY;
ALTER TABLE principals ENABLE ROW LEVEL SECURITY;
ALTER TABLE principals FORCE ROW LEVEL SECURITY;
ALTER TABLE roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE roles FORCE ROW LEVEL SECURITY;
ALTER TABLE capabilities ENABLE ROW LEVEL SECURITY;
ALTER TABLE capabilities FORCE ROW LEVEL SECURITY;
ALTER TABLE principal_roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE principal_roles FORCE ROW LEVEL SECURITY;
ALTER TABLE role_capabilities ENABLE ROW LEVEL SECURITY;
ALTER TABLE role_capabilities FORCE ROW LEVEL SECURITY;

CREATE POLICY workspaces_workspace_isolation ON workspaces
USING (id = vestrace_current_workspace_id())
WITH CHECK (id = vestrace_current_workspace_id());

CREATE POLICY principals_workspace_isolation ON principals
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE POLICY roles_workspace_isolation ON roles
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE POLICY capabilities_workspace_isolation ON capabilities
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE POLICY principal_roles_workspace_isolation ON principal_roles
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE POLICY role_capabilities_workspace_isolation ON role_capabilities
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());
