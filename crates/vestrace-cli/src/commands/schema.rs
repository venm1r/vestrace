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
                                        "items": { "$ref": "#/components/schemas/Model" }
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
                                        "items": { "$ref": "#/components/schemas/Provider" }
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "summary": "Register a provider",
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
                                "schema": { "$ref": "#/components/schemas/CreateProviderRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Provider registered",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Provider" }
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
                    "summary": "List connections (not yet implemented)",
                    "tags": ["connections"],
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
                        "source_event_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["kind", "content"]
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
                                    "score": { "type": "number" },
                                    "channel": { "type": "string" },
                                    "explanation": { "type": "string" }
                                }
                            }
                        }
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
