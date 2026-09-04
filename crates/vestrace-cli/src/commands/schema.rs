use clap::ValueEnum;
use vestrace_mcp::McpServer;

#[derive(Clone, Debug, ValueEnum)]
pub enum SchemaFormat {
    Http,
    Mcp,
}

pub fn run(format: &SchemaFormat) -> anyhow::Result<()> {
    match format {
        SchemaFormat::Http => {
            let spec = openapi_v1();
            let json = serde_json::to_string_pretty(&spec)
                .map_err(|e| anyhow::anyhow!("failed to serialize OpenAPI spec: {e}"))?;
            println!("{json}");
        }
        SchemaFormat::Mcp => {
            let tools = McpServer::tool_definitions();
            let spec = serde_json::json!({
                "version": "1.0",
                "tools": tools.iter().map(|t| serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })).collect::<Vec<_>>()
            });
            let json = serde_json::to_string_pretty(&spec)
                .map_err(|e| anyhow::anyhow!("failed to serialize MCP tools spec: {e}"))?;
            println!("{json}");
        }
    }
    Ok(())
}

fn openapi_v1() -> serde_json::Value {
    let governed_mutation_headers = serde_json::json!([
        {
            "name": "x-workspace-id",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
        },
        {
            "name": "x-principal-id",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
        },
        {
            "name": "x-request-id",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
        },
        {
            "name": "Idempotency-Key",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "minLength": 1, "maxLength": 200 }
        }
    ]);
    serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Vestrace API",
            "version": "1.0.0",
            "description": "Cognitive runtime foundation — memory, retrieval, routing, execution, and diagnostics"
        },
        "servers": [
            { "url": "/" }
        ],
        "paths": {
            "/health/live": {
                "get": {
                    "summary": "Liveness probe",
                    "description": "Returns 200 if the process is alive",
                    "responses": {
                        "200": {
                            "description": "Alive",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "status": { "type": "string", "enum": ["ok"] }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/health/ready": {
                "get": {
                    "summary": "Readiness probe",
                    "description": "Returns 200 if the service is ready to accept requests",
                    "responses": {
                        "200": {
                            "description": "Ready",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "status": { "type": "string", "enum": ["ok"] }
                                        }
                                    }
                                }
                            }
                        },
                        "503": {
                            "description": "Not ready",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/metrics": {
                "get": {
                    "summary": "Prometheus metrics",
                    "description": "Returns Prometheus-format text metrics (no sensitive labels)",
                    "responses": {
                        "200": {
                            "description": "Metrics text",
                            "content": {
                                "text/plain": {
                                    "schema": { "type": "string" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/runs": {
                "get": {
                    "summary": "List runs",
                    "tags": ["runs"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of runs",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/Run" }
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "summary": "Create a run",
                    "tags": ["runs"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-request-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-correlation-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateRunRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Run created",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Run" }
                                }
                            }
                        },
                        "400": {
                            "description": "Invalid request",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/runs/{id}": {
                "get": {
                    "summary": "Get a run by ID",
                    "tags": ["runs"],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Run",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Run" }
                                }
                            }
                        },
                        "404": {
                            "description": "Not found",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/runs/{id}/approve": {
                "post": {
                    "summary": "Approve a run (not yet implemented)",
                    "tags": ["runs"],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "501": {
                            "description": "Not implemented",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/events": {
                "post": {
                    "summary": "Record an event",
                    "tags": ["events"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-request-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateEventRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Event recorded",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Event" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/memories": {
                "post": {
                    "summary": "Create a memory (derived from events)",
                    "tags": ["memories"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-request-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateMemoryRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Memory created",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Memory" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/memories/{id}": {
                "get": {
                    "summary": "Get a memory by ID",
                    "tags": ["memories"],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Memory",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Memory" }
                                }
                            }
                        },
                        "404": {
                            "description": "Not found",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/memories/{id}/revisions": {
                "post": {
                    "summary": "Create a revision of an existing memory",
                    "tags": ["memories"],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "idempotency-key",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "if-match",
                            "in": "header",
                            "required": true,
                            "description": "The active memory revision number expected by the caller",
                            "schema": { "type": "integer", "minimum": 0 }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ReviseMemoryRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Memory revision created",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Memory" }
                                }
                            }
                        },
                        "409": {
                            "description": "Revision conflict",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/retrieval/search": {
                "post": {
                    "summary": "Search memories using hybrid retrieval",
                    "tags": ["retrieval"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/RetrievalRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Retrieval results",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/RetrievalResponse" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/agents": {
                "get": {
                    "summary": "List agents",
                    "tags": ["agents"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of agents",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/Agent" }
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "summary": "Create an agent",
                    "tags": ["agents"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateAgentRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Agent created",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Agent" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/models": {
                "get": {
                    "summary": "List models",
                    "tags": ["models"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of models",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/GovernedModel" }
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "summary": "Register a model",
                    "tags": ["models"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateModelRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Model registered",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Model" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/connections/{id}/revisions": {
                "post": {
                    "summary": "Create an immutable connection revision",
                    "tags": ["provider-connections"],
                    "parameters": governed_mutation_headers.clone(),
                    "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreateConnectionRevisionRequest" } } } },
                    "responses": { "201": { "description": "Connection revision created", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GovernedConnection" } } } } }
                }
            },
            "/v1/connections/{id}/admission-policies": {
                "post": {
                    "summary": "Publish the connection admission policy that governs every dispatch",
                    "tags": ["provider-connections"],
                    "parameters": governed_mutation_headers.clone(),
                    "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/PublishConnectionAdmissionPolicyRequest" } } } },
                    "responses": {
                        "201": { "description": "Admission policy published", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GovernedMutationReceipt" } } } },
                        "409": { "description": "The stated head version is not the current one" }
                    }
                }
            },
            "/v1/connections/{id}/qualifications": {
                "post": {
                    "summary": "Request governed connection qualification",
                    "tags": ["provider-connections"],
                    "parameters": governed_mutation_headers.clone(),
                    "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/QualificationRequest" } } } },
                    "responses": { "202": { "description": "Qualification requested", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Qualification" } } } } }
                }
            },
            "/v1/connections/{id}/credentials": {
                "post": {
                    "summary": "Prepare a governed connection credential",
                    "tags": ["provider-connections"],
                    "parameters": governed_mutation_headers.clone(),
                    "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreateConnectionCredentialRequest" } } } },
                    "responses": { "202": { "description": "Credential prepared", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CredentialLifecycle" } } } } }
                }
            },
            "/v1/connections/{id}/credentials/{revision_id}/abandon": {
                "post": {
                    "summary": "Abandon a candidate credential after durable erasure",
                    "tags": ["provider-connections"], "parameters": governed_mutation_headers.clone(),
                    "responses": { "202": { "description": "Credential abandonment resumed", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CredentialLifecycle" } } } } }
                }
            },
            "/v1/connections/{id}/credentials/{revision_id}/activate": {
                "post": {
                    "summary": "Activate a governed credential revision",
                    "tags": ["provider-connections"], "parameters": governed_mutation_headers.clone(),
                    "responses": { "200": { "description": "Credential activated", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CredentialLifecycle" } } } } }
                }
            },
            "/v1/connections/{id}/credentials/{revision_id}/revoke": {
                "post": {
                    "summary": "Revoke a governed credential revision",
                    "tags": ["provider-connections"], "parameters": governed_mutation_headers.clone(),
                    "responses": { "200": { "description": "Credential revoked", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CredentialLifecycle" } } } } }
                }
            },
            "/v1/models/{id}/revisions": {
                "post": {
                    "summary": "Create an immutable model revision",
                    "tags": ["provider-models"], "parameters": governed_mutation_headers.clone(),
                    "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreateModelRevisionRequest" } } } },
                    "responses": { "201": { "description": "Model revision created", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GovernedModel" } } } } }
                }
            },
            "/v1/models/{id}/qualifications": {
                "post": {
                    "summary": "Request governed model qualification",
                    "tags": ["provider-models"], "parameters": governed_mutation_headers.clone(),
                    "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/QualificationRequest" } } } },
                    "responses": { "202": { "description": "Qualification requested", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Qualification" } } } } }
                }
            },
            "/v1/providers": {
                "get": {
                    "summary": "List providers",
                    "tags": ["models"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of providers",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/GovernedProvider" }
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "summary": "Refuse legacy provider registry creation",
                    "tags": ["models"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "403": {
                            "description": "Legacy provider registry retired",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/skills": {
                "get": {
                    "summary": "List skills",
                    "tags": ["skills"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of skills",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/Skill" }
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "summary": "Create a skill",
                    "tags": ["skills"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateSkillRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Skill created",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Skill" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/workflows": {
                "get": {
                    "summary": "List workflows",
                    "tags": ["workflows"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of workflows",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/Workflow" }
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "summary": "Create a workflow",
                    "tags": ["workflows"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateWorkflowRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Workflow created",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Workflow" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/workflows/{id}": {
                "get": {
                    "summary": "Get a workflow by ID",
                    "tags": ["workflows"],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Workflow",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Workflow" }
                                }
                            }
                        },
                        "404": {
                            "description": "Not found",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/workflow-executions": {
                "post": {
                    "summary": "Start a workflow execution",
                    "tags": ["workflow-executions"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/StartWorkflowExecutionRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Workflow execution started",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/WorkflowExecution" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/workflow-executions/{id}": {
                "get": {
                    "summary": "Get a workflow execution by ID",
                    "tags": ["workflow-executions"],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Workflow execution",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/WorkflowExecution" }
                                }
                            }
                        },
                        "404": {
                            "description": "Not found",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/routing/decisions": {
                "post": {
                    "summary": "Route a model execution request",
                    "tags": ["routing"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/RouteRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Routing decision",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/RoutingDecision" }
                                }
                            }
                        }
                    }
                },
                "get": {
                    "summary": "List routing decisions",
                    "tags": ["routing"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of routing decisions",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/RoutingDecision" }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/executions": {
                "post": {
                    "summary": "Record an execution",
                    "tags": ["executions"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/RecordExecutionRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Execution recorded",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ExecutionRecord" }
                                }
                            }
                        }
                    }
                },
                "get": {
                    "summary": "List executions",
                    "tags": ["executions"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of executions",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/ExecutionRecord" }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/evaluations": {
                "post": {
                    "summary": "Record an evaluation",
                    "tags": ["evaluations"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateEvaluationRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Evaluation recorded",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Evaluation" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/evaluations/{id}": {
                "get": {
                    "summary": "Get an evaluation by ID",
                    "tags": ["evaluations"],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Evaluation",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Evaluation" }
                                }
                            }
                        },
                        "404": {
                            "description": "Not found",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/artifacts": {
                "get": {
                    "summary": "List artifacts (not yet implemented)",
                    "tags": ["artifacts"],
                    "responses": {
                        "501": {
                            "description": "Not implemented",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/triggers": {
                "get": {
                    "summary": "List triggers (not yet implemented)",
                    "tags": ["triggers"],
                    "responses": {
                        "501": {
                            "description": "Not implemented",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/connections": {
                "get": {
                    "summary": "List governed connection projections",
                    "tags": ["provider-connections"],
                    "parameters": [
                        {
                            "name": "x-workspace-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        },
                        {
                            "name": "x-principal-id",
                            "in": "header",
                            "required": true,
                            "schema": { "type": "string", "format": "uuid" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of governed connection projections",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/GovernedConnection" }
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "summary": "Create a governed connection",
                    "tags": ["provider-connections"],
                    "parameters": governed_mutation_headers.clone(),
                    "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreateConnectionRequest" } } } },
                    "responses": { "201": { "description": "Connection created", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GovernedConnection" } } } }, "409": { "description": "Idempotency or lifecycle conflict", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ApiError" } } } } }
                }
            },
            "/v1/embedding-jobs/{id}/acknowledge-unknown": {
                "post": {
                    "summary": "Acknowledge a possible duplicate embedding charge and create its successor",
                    "tags": ["embedding-jobs"],
                    "parameters": governed_mutation_headers.clone(),
                    "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/AcknowledgeEmbeddingJobUnknownRequest" } } } },
                    "responses": {
                        "201": { "description": "Successor accepted", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GovernedMutationReceipt" } } } },
                        "409": { "description": "Embedding job acceptance refused", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ApiError" } } } }
                    }
                }
            },
            "/v1/embedding-transitions/{id}/acknowledge-carry": {
                "post": {
                    "summary": "Acknowledge a carried transition ambiguity and create its one successor",
                    "tags": ["embedding-transitions"],
                    "parameters": governed_mutation_headers.clone(),
                    "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/AcknowledgeEmbeddingTransitionCarryRequest" } } } },
                    "responses": {
                        "201": { "description": "Carried successor accepted", "content": { "application/json": { "schema": { "type": "object" } } } },
                        "409": { "description": "Carry acknowledgement refused", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ApiError" } } } }
                    }
                }
            },
            "/v1/audit": {
                "get": {
                    "summary": "List audit events (not yet implemented)",
                    "tags": ["audit"],
                    "responses": {
                        "501": {
                            "description": "Not implemented",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/metrics/summary": {
                "get": {
                    "summary": "Get metrics summary (not yet implemented)",
                    "tags": ["metrics"],
                    "responses": {
                        "501": {
                            "description": "Not implemented",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/system/health": {
                "get": {
                    "summary": "System health (not yet implemented)",
                    "tags": ["system"],
                    "responses": {
                        "501": {
                            "description": "Not implemented",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/v1/profile": {
                "get": {
                    "summary": "Get workspace profile (not yet implemented)",
                    "tags": ["profile"],
                    "responses": {
                        "501": {
                            "description": "Not implemented",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/ag-ui/endpoints": {
                "get": {
                    "summary": "AG-UI endpoints (not yet implemented)",
                    "tags": ["ag-ui"],
                    "responses": {
                        "501": {
                            "description": "Not implemented",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "summary": "Create AG-UI endpoint (not yet implemented)",
                    "tags": ["ag-ui"],
                    "responses": {
                        "501": {
                            "description": "Not implemented",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/ag-ui/events/stream": {
                "get": {
                    "summary": "AG-UI event stream (not yet implemented)",
                    "tags": ["ag-ui"],
                    "responses": {
                        "501": {
                            "description": "Not implemented",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            },
            "/ag-ui/run": {
                "post": {
                    "summary": "AG-UI run (not yet implemented)",
                    "tags": ["ag-ui"],
                    "responses": {
                        "501": {
                            "description": "Not implemented",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ApiError" }
                                }
                            }
                        }
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "ApiError": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "string" },
                        "message": { "type": "string" }
                    },
                    "required": ["code", "message"]
                },
                "AcknowledgeEmbeddingJobUnknownRequest": {
                    "type": "object",
                    "properties": {
                        "embedding_job_id": { "type": "string", "format": "uuid" },
                        "expected_predecessor_version": { "type": "integer", "format": "int64", "minimum": 1 },
                        "successor_embedding_job_id": { "type": "string", "format": "uuid" },
                        "successor_model_request_evidence_id": { "type": "string", "format": "uuid" },
                        "space_registration_id": { "type": "string", "format": "uuid" },
                        "model_binding_snapshot_id": { "type": "string", "format": "uuid" },
                        "kind": { "type": "string", "enum": ["retrieval_query", "delivery", "rebuild"] },
                        "effect_intent": { "$ref": "#/components/schemas/EmbeddingEffectIntent" }
                    },
                    "required": ["embedding_job_id", "expected_predecessor_version", "successor_embedding_job_id", "successor_model_request_evidence_id", "space_registration_id", "model_binding_snapshot_id", "kind", "effect_intent"]
                },
                "AcknowledgeEmbeddingTransitionCarryRequest": {
                    "type": "object",
                    "properties": {
                        "transition_id": { "type": "string", "format": "uuid" },
                        "carry_id": { "type": "string", "format": "uuid" },
                        "predecessor_embedding_job_id": { "type": "string", "format": "uuid" },
                        "predecessor_version": { "type": "integer", "minimum": 1 },
                        "predecessor_transition_batch_id": { "type": "string", "format": "uuid" },
                        "successor_transition_batch_id": { "type": "string", "format": "uuid" },
                        "successor_embedding_job_id": { "type": "string", "format": "uuid" },
                        "space_registration_id": { "type": "string", "format": "uuid" },
                        "kind": { "type": "string", "enum": ["retrieval_query", "delivery", "rebuild"] },
                        "model_binding_snapshot_id": { "type": "string", "format": "uuid" },
                        "external_effect_id": { "type": "string", "format": "uuid" },
                        "model_request_evidence_id": { "type": "string", "format": "uuid" },
                        "mappings": { "type": "array", "minItems": 1, "items": { "$ref": "#/components/schemas/EmbeddingTransitionCarryMapping" } }
                    },
                    "required": ["transition_id", "carry_id", "predecessor_embedding_job_id", "predecessor_version", "predecessor_transition_batch_id", "successor_transition_batch_id", "successor_embedding_job_id", "space_registration_id", "kind", "model_binding_snapshot_id", "external_effect_id", "model_request_evidence_id", "mappings"]
                },
                "EmbeddingTransitionCarryMapping": {
                    "type": "object",
                    "properties": {
                        "old_recipe_ordinal": { "type": "integer", "minimum": 0 },
                        "old_input_ordinal": { "type": "integer", "minimum": 0 },
                        "new_recipe_ordinal": { "type": "integer", "minimum": 0 },
                        "new_input_ordinal": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["old_recipe_ordinal", "old_input_ordinal", "new_recipe_ordinal", "new_input_ordinal"]
                },
                "EmbeddingEffectIntent": {
                    "type": "object",
                    "properties": {
                        "execution_ref": { "type": "string" },
                        "adapter": { "type": "string" },
                        "operation": { "type": "string" },
                        "target": { "type": "string" },
                        "normalized_arguments_digest": { "type": "string" },
                        "expected_effect": { "type": "string" },
                        "preconditions": { "type": "array", "minItems": 1, "items": { "$ref": "#/components/schemas/EmbeddingEffectPrecondition" } },
                        "precondition_digest": { "type": "string" },
                        "risk": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
                        "reversibility": { "type": "string", "enum": ["reversible", "compensatable", "irreversible", "unknown"] },
                        "idempotency_profile": { "type": "string", "enum": ["none", "provider_key", "conditional", "unknown"] },
                        "delivery_semantics": { "type": "string", "enum": ["at_most_once", "at_least_once", "effectively_once", "unknown"] },
                        "required_capability": { "type": "string" },
                        "budget_reservation_ref": { "type": ["string", "null"] },
                        "policy_decision_ref": { "type": ["string", "null"] }
                    },
                    "required": ["execution_ref", "adapter", "operation", "target", "normalized_arguments_digest", "expected_effect", "preconditions", "precondition_digest", "risk", "reversibility", "idempotency_profile", "delivery_semantics", "required_capability"]
                },
                "EmbeddingEffectPrecondition": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "expected_value": { "type": "string" }
                    },
                    "required": ["name", "expected_value"]
                },
                "Run": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "status": { "type": "string", "enum": ["queued", "running", "waiting", "succeeded", "failed", "cancelled"] },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "CreateRunRequest": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "format": "uuid" },
                        "input": { "type": "string" }
                    },
                    "required": ["agent_id", "input"]
                },
                "Event": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "session_id": { "type": "string", "format": "uuid" },
                        "event_type": { "type": "string" },
                        "actor": { "type": "object" },
                        "payload": { "type": "object" },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "CreateEventRequest": {
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "format": "uuid" },
                        "event_type": { "type": "string" },
                        "actor": { "type": "object" },
                        "payload": { "type": "object" }
                    },
                    "required": ["event_type", "payload"]
                },
                "Memory": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "kind": { "type": "string", "enum": ["fact", "decision", "task", "procedure", "observation", "outcome", "summary"] },
                        "status": { "type": "string", "enum": ["active", "superseded", "deleted"] },
                        "active_revision_id": { "type": "string", "format": "uuid" },
                        "classification": { "type": ["string", "null"] },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    }
                },
                "CreateMemoryRequest": {
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string" },
                        "content": { "type": "string" },
                        "confidence": { "type": "number" },
                        "importance": { "type": "number" },
                        "source_event_id": { "type": "string", "format": "uuid" },
                        "classification": { "type": ["string", "null"] }
                    },
                    "required": ["kind", "content"]
                },
                "ReviseMemoryRequest": {
                    "type": "object",
                    "properties": {
                        "content": { "type": "string" },
                        "confidence": { "type": "number" },
                        "importance": { "type": "number" },
                        "source_event_id": { "type": "string", "format": "uuid" },
                        "change_reason": { "type": ["string", "null"] },
                        "classification": {
                            "type": ["string", "null"],
                            "description": "Omit to inherit the active label; null requests clearing and is refused because no label transition mechanism exists"
                        }
                    },
                    "required": ["content", "confidence", "importance", "source_event_id"]
                },
                "RetrievalRequest": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "token_budget": { "type": "integer" },
                        "session_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["query"]
                },
                "RetrievalResponse": {
                    "type": "object",
                    "properties": {
                        "candidates": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "memory_id": { "type": "string", "format": "uuid" },
                                    "revision_id": { "type": "string", "format": "uuid" },
                                    "score": { "type": "number" },
                                    "channel": { "type": "string" },
                                    "explanation": { "type": "string" },
                                    "source_classification": { "type": ["string", "null"] }
                                }
                            }
                        },
                        "withheld": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "memory_id": { "type": "string", "format": "uuid" },
                                    "revision_id": { "type": "string", "format": "uuid" },
                                    "reason": {
                                        "type": "string",
                                        "enum": [
                                            "revision_not_found",
                                            "classification_not_admissible"
                                        ]
                                    },
                                    "classification": { "type": ["string", "null"] }
                                },
                                "required": ["memory_id", "revision_id", "reason"]
                            }
                        },
                        "retrieval_policy_version": { "type": "string", "minLength": 1 }
                    }
                },
                "Agent": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "current_revision": { "type": "integer" },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "CreateAgentRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "role": { "type": "string", "enum": ["specialist", "orchestrator"] },
                        "instructions": { "type": "string" }
                    },
                    "required": ["name", "role", "instructions"]
                },
                "Model": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "provider_id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "context_window": { "type": "integer" },
                        "cost": { "$ref": "#/components/schemas/ModelCostProfile" },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "ModelCostProfile": {
                    "type": "object",
                    "properties": {
                        "input_cost_per_mtoken": { "type": "number" },
                        "output_cost_per_mtoken": { "type": "number" }
                    }
                },
                "CreateModelRequest": {
                    "type": "object",
                    "properties": {
                        "provider_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "context_window": { "type": "integer" },
                        "input_cost_per_mtoken": { "type": "number" },
                        "output_cost_per_mtoken": { "type": "number" }
                    },
                    "required": ["provider_id", "name", "context_window"]
                },
                "GovernedConnection": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "revision_id": { "type": ["string", "null"], "format": "uuid" },
                        "state": { "type": "string" },
                        "qualification_state": { "type": ["string", "null"] },
                        "blockers": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["id", "revision_id", "state", "qualification_state", "blockers"]
                },
                "GovernedModel": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "revision_id": { "type": ["string", "null"], "format": "uuid" },
                        "state": { "type": "string" },
                        "qualification_state": { "type": ["string", "null"] },
                        "blockers": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["id", "revision_id", "state", "qualification_state", "blockers"]
                },
                "GovernedProvider": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "state": { "type": "string" },
                        "blockers": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["id", "state", "blockers"]
                },
                "Qualification": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "state": { "type": "string" },
                        "profile_revision": { "type": "string" },
                        "blockers": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["id", "state", "blockers"]
                },
                "CredentialLifecycle": {
                    "type": "object",
                    "properties": {
                        "connection_id": { "type": "string", "format": "uuid" },
                        "revision_id": { "type": "string", "format": "uuid" },
                        "state": { "type": "string" }
                    },
                    "required": ["connection_id", "revision_id", "state"]
                },
                "CreateConnectionRequest": {
                    "type": "object",
                    "properties": { "name": { "type": "string" }, "connection_kind": { "type": "string" }, "runtime_base_url": { "type": "string", "format": "uri" } },
                    "required": ["name", "connection_kind", "runtime_base_url"]
                },
                "CreateConnectionRevisionRequest": {
                    "type": "object",
                    "properties": { "connection_kind": { "type": "string" }, "runtime_base_url": { "type": "string", "format": "uri" }, "expected_version": { "type": "integer", "minimum": 0 } },
                    "required": ["connection_kind", "runtime_base_url", "expected_version"]
                },
                "PublishConnectionAdmissionPolicyRequest": {
                    "type": "object",
                    "properties": {
                        "connection_id": { "type": "string", "format": "uuid" },
                        "policy_revision_id": { "type": "string", "format": "uuid" },
                        "max_in_flight": { "type": "integer", "minimum": 1, "maximum": 64 },
                        "requests_per_60_seconds": { "type": "integer", "minimum": 1, "maximum": 60000 },
                        "queue_wait_timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 300 },
                        "provider_throttle_cap_seconds": { "type": "integer", "minimum": 1, "maximum": 900 },
                        "expected_head_version": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["connection_id", "policy_revision_id", "max_in_flight", "requests_per_60_seconds", "queue_wait_timeout_seconds", "provider_throttle_cap_seconds", "expected_head_version"]
                },
                "GovernedMutationReceipt": {
                    "type": "object",
                    "properties": {
                        "audit_event_id": { "type": "string", "format": "uuid" },
                        "idempotency_key": { "type": "string", "nullable": true },
                        "outbox_message_ids": { "type": "array", "items": { "type": "string", "format": "uuid" } }
                    },
                    "required": ["audit_event_id", "outbox_message_ids"]
                },
                "QualificationRequest": {
                    "type": "object",
                    "properties": { "profile_revision": { "type": "string" } },
                    "required": ["profile_revision"]
                },
                "CreateConnectionCredentialRequest": {
                    "type": "object",
                    "properties": { "credential": { "type": "string", "writeOnly": true, "minLength": 1 } },
                    "required": ["credential"]
                },
                "CreateModelRevisionRequest": {
                    "type": "object",
                    "properties": { "model_name": { "type": "string" }, "connection_revision_id": { "type": "string", "format": "uuid" }, "expected_version": { "type": "integer", "minimum": 0 } },
                    "required": ["model_name", "connection_revision_id", "expected_version"]
                },
                "Provider": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "locality": { "type": "string", "enum": ["local", "remote"] },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "CreateProviderRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "locality": { "type": "string", "enum": ["local", "remote"] }
                    },
                    "required": ["name", "locality"]
                },
                "Skill": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "current_revision": { "type": "integer" },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "CreateSkillRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "kind": { "type": "string", "enum": ["prompt", "http", "mcp_tool", "composite", "human"] },
                        "implementation": { "type": "object" }
                    },
                    "required": ["name", "kind"]
                },
                "Workflow": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "current_revision": { "type": "integer" },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "CreateWorkflowRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "nodes": { "type": "array", "items": { "type": "object" } },
                        "transitions": { "type": "array", "items": { "type": "object" } }
                    },
                    "required": ["name", "nodes", "transitions"]
                },
                "WorkflowExecution": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "workflow_id": { "type": "string", "format": "uuid" },
                        "workflow_revision": { "type": "integer" },
                        "status": { "type": "string", "enum": ["queued", "running", "waiting", "succeeded", "failed", "cancelled"] },
                        "attempt": { "type": "integer" },
                        "started_at": { "type": "string", "format": "date-time" },
                        "completed_at": { "type": "string", "format": "date-time" }
                    }
                },
                "StartWorkflowExecutionRequest": {
                    "type": "object",
                    "properties": {
                        "workflow_id": { "type": "string", "format": "uuid" },
                        "attempt": { "type": "integer" }
                    },
                    "required": ["workflow_id"]
                },
                "RoutingDecision": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "strategy": { "type": "string", "enum": ["balanced", "quality_first", "cost_first", "privacy_first"] },
                        "selected": { "type": "object" },
                        "fallback_used": { "type": "boolean" },
                        "rationale": { "type": "string" },
                        "rejected": { "type": "array", "items": { "type": "object" } },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "RouteRequest": {
                    "type": "object",
                    "properties": {
                        "task_type": { "type": "string" },
                        "min_quality": { "type": "number" },
                        "max_cost_per_mtoken": { "type": "number" },
                        "context_tokens": { "type": "integer" },
                        "privacy_local_only": { "type": "boolean" },
                        "data_sensitivity": { "type": "string", "enum": ["public", "internal", "restricted", "confidential"] },
                        "remote_transfer_approved": { "type": "boolean" }
                    },
                    "required": ["task_type"]
                },
                "ExecutionRecord": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "step_execution_id": { "type": "string", "format": "uuid" },
                        "kind": { "type": "string", "enum": ["output", "input", "intermediate"] },
                        "content_ref": { "type": "string" },
                        "content_type": { "type": "string" },
                        "byte_size": { "type": "integer" },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "RecordExecutionRequest": {
                    "type": "object",
                    "properties": {
                        "step_execution_id": { "type": "string", "format": "uuid" },
                        "kind": { "type": "string" },
                        "content_ref": { "type": "string" },
                        "content_type": { "type": "string" }
                    },
                    "required": ["step_execution_id", "kind", "content_ref"]
                },
                "Evaluation": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "workspace_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "model_id": { "type": "string", "format": "uuid" },
                        "score": { "type": "number" },
                        "summary": { "type": "string" },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "CreateEvaluationRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "model_id": { "type": "string", "format": "uuid" },
                        "score": { "type": "number" },
                        "summary": { "type": "string" }
                    },
                    "required": ["name"]
                }
            }
        }
    })
}

#[cfg(test)]
mod memory_classification_schema_tests {
    use super::openapi_v1;

    #[test]
    fn memory_create_and_response_schemas_expose_classification() {
        let schema = openapi_v1();

        assert_eq!(
            schema["components"]["schemas"]["CreateMemoryRequest"]["properties"]["classification"]
                ["type"],
            serde_json::json!(["string", "null"])
        );
        assert_eq!(
            schema["components"]["schemas"]["Memory"]["properties"]["classification"]["type"],
            serde_json::json!(["string", "null"])
        );
    }

    #[test]
    fn memory_revision_schema_exposes_concurrency_and_classification_contract() {
        let schema = openapi_v1();
        let operation = &schema["paths"]["/v1/memories/{id}/revisions"]["post"];

        assert_eq!(
            operation["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/ReviseMemoryRequest"
        );
        assert!(
            operation["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .any(|parameter| parameter["name"] == "if-match" && parameter["required"] == true)
        );
        assert_eq!(
            schema["components"]["schemas"]["ReviseMemoryRequest"]["properties"]["classification"]
                ["type"],
            serde_json::json!(["string", "null"])
        );
        assert!(
            !schema["components"]["schemas"]["ReviseMemoryRequest"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "classification")
        );
    }

    #[test]
    fn retrieval_schema_exposes_classification_and_structured_withholding() {
        let schema = openapi_v1();
        let response = &schema["components"]["schemas"]["RetrievalResponse"];
        let candidate = &response["properties"]["candidates"]["items"];
        let withheld = &response["properties"]["withheld"]["items"];

        assert_eq!(
            candidate["properties"]["source_classification"]["type"],
            serde_json::json!(["string", "null"])
        );
        assert_eq!(withheld["properties"]["memory_id"]["format"], "uuid");
        assert_eq!(withheld["properties"]["revision_id"]["format"], "uuid");
        assert_eq!(
            response["properties"]["retrieval_policy_version"]["minLength"],
            1
        );
        assert_eq!(
            withheld["properties"]["reason"]["enum"],
            serde_json::json!(["revision_not_found", "classification_not_admissible"])
        );
    }
}
