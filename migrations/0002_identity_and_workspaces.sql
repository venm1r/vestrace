CREATE TABLE workspaces (
    id UUID PRIMARY KEY,
    slug TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX workspaces_slug_ci_key ON workspaces ((lower(slug)));

CREATE TABLE principals (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    identifier TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT principals_workspace_fkey
        FOREIGN KEY (workspace_id)
        REFERENCES workspaces (id)
        ON DELETE CASCADE,
    CONSTRAINT principals_workspace_id_id_key
        UNIQUE (workspace_id, id),
    CONSTRAINT principals_workspace_identifier_key
        UNIQUE (workspace_id, identifier)
);

CREATE TABLE roles (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT roles_workspace_fkey
        FOREIGN KEY (workspace_id)
        REFERENCES workspaces (id)
        ON DELETE CASCADE,
    CONSTRAINT roles_workspace_id_id_key
        UNIQUE (workspace_id, id),
    CONSTRAINT roles_workspace_name_key
        UNIQUE (workspace_id, name)
);

CREATE TABLE capabilities (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT capabilities_workspace_fkey
        FOREIGN KEY (workspace_id)
        REFERENCES workspaces (id)
        ON DELETE CASCADE,
    CONSTRAINT capabilities_workspace_id_id_key
        UNIQUE (workspace_id, id),
    CONSTRAINT capabilities_workspace_name_key
        UNIQUE (workspace_id, name)
);

CREATE TABLE principal_roles (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    role_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT principal_roles_workspace_fkey
        FOREIGN KEY (workspace_id)
        REFERENCES workspaces (id)
        ON DELETE CASCADE,
    CONSTRAINT principal_roles_principal_fkey
        FOREIGN KEY (workspace_id, principal_id)
        REFERENCES principals (workspace_id, id)
        ON DELETE CASCADE,
    CONSTRAINT principal_roles_role_fkey
        FOREIGN KEY (workspace_id, role_id)
        REFERENCES roles (workspace_id, id)
        ON DELETE CASCADE,
    CONSTRAINT principal_roles_workspace_principal_role_key
        UNIQUE (workspace_id, principal_id, role_id)
);

CREATE TABLE role_capabilities (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    role_id UUID NOT NULL,
    capability_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT role_capabilities_workspace_fkey
        FOREIGN KEY (workspace_id)
        REFERENCES workspaces (id)
        ON DELETE CASCADE,
    CONSTRAINT role_capabilities_role_fkey
        FOREIGN KEY (workspace_id, role_id)
        REFERENCES roles (workspace_id, id)
        ON DELETE CASCADE,
    CONSTRAINT role_capabilities_capability_fkey
        FOREIGN KEY (workspace_id, capability_id)
        REFERENCES capabilities (workspace_id, id)
        ON DELETE CASCADE,
    CONSTRAINT role_capabilities_workspace_role_capability_key
        UNIQUE (workspace_id, role_id, capability_id)
);
