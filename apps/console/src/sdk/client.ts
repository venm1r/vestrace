export interface ApiErrorBody {
  code: string;
  message: string;
}

export class ApiRequestError extends Error {
  constructor(
    public readonly status: number,
    public readonly body: ApiErrorBody,
  ) {
    super(body.message);
    this.name = 'ApiRequestError';
  }

  /** The backend answers 501 for surfaces that are reserved but unimplemented. */
  get isNotImplemented(): boolean {
    return this.status === 501 || this.body.code === 'not_implemented';
  }

  /** Missing or malformed workspace/principal identity headers. */
  get isIdentityRejected(): boolean {
    return this.status === 400 && /x-(workspace|principal)-id/i.test(this.body.message);
  }
}

/** Mirrors `vestrace_domain::run::RunStatus` (serde `snake_case`). */
export type RunStatus =
  | 'created'
  | 'preparing'
  | 'running'
  | 'waiting_for_input'
  | 'waiting_for_approval'
  | 'waiting_for_dependency'
  | 'paused'
  | 'paused_policy_changed'
  | 'succeeded'
  | 'succeeded_with_warnings'
  | 'partial'
  | 'failed'
  | 'cancelled'
  | 'expired';

export interface RunItem {
  id: string;
  title: string;
  status: RunStatus;
  version: number;
  created_at: string;
  updated_at: string;
}

export interface CreateRunPayload {
  title: string;
}

/** Mirrors `api::agents::AgentResponse`. */
export interface AgentItem {
  id: string;
  name: string;
  description: string;
  system_prompt: string;
}

/** Mirrors the JSON emitted by `api::workflows::list_workflows`. */
export interface WorkflowItem {
  id: string;
  name: string;
  current_revision: number;
  created_at: string;
}

/** Mirrors `api::models::ModelResponse`. */
export interface ModelItem {
  id: string;
  provider_id: string;
  model_name: string;
  context_window: number;
  input_cost_per_mtoken: number;
  output_cost_per_mtoken: number;
}

/** Mirrors `api::models::ProviderResponse`. */
export interface ProviderItem {
  id: string;
  name: string;
  locality: string;
}

/** Mirrors the JSON emitted by `api::evaluations::list_evaluations`. */
export interface EvaluationItem {
  id: string;
  model_id: string | null;
  name: string;
  status: string | null;
  score: number | null;
  summary: string | null;
  created_at: string;
}

/** Mirrors the JSON emitted by `api::artifacts::list_artifacts`.
 *
 * These are the field names the adapter actually sends. The previous shape
 * declared `kind`, `size` and `checksum`, none of which exist in the response,
 * so all three columns rendered blank — including the digest, which is the
 * whole point of showing an artifact. It went unnoticed because no artifact
 * existed until a run step produced one, and an empty table hides a field
 * mismatch perfectly.
 */
export interface ArtifactItem {
  id: string;
  name: string;
  status: string;
  created_at: string;
  media_type: string;
  size_bytes: number;
  content_sha256: string;
  revision_number: number;
}

/*
 * The surfaces below are reserved by the HTTP adapter but answer 501 in this
 * foundation. The shapes describe the intended contract; the console renders an
 * explicit "not implemented" state until the endpoints exist.
 */

export interface TriggerItem {
  id: string;
  name: string;
  type: string;
  target: string;
  status: string;
}

export interface ConnectionItem {
  id: string;
  name: string;
  type: string;
  status: string;
  latency: string;
}

export interface AuditEventItem {
  id: string;
  timestamp: string;
  actor: string;
  action: string;
  resource: string;
}

/**
 * Only values the runtime can actually observe. An earlier shape also declared
 * an average latency and a spent budget; neither is measured or accounted
 * anywhere, so they were removed rather than filled with plausible numbers.
 */
export interface MetricsSummary {
  live_runs: number;
  runs_today: number;
  registered_agents: number;
  registered_models: number;
  /** Per-run ceiling in micro-units, or null when no cap is expressed. */
  run_budget_cap_micros: number | null;
}

export interface SystemHealthFinding {
  /** The invariant's id, e.g. `outbox.backlog_within_budget`. */
  code: string;
  /**
   * The version of the invariant that produced this finding. Two findings with
   * the same code and different versions came from different definitions of the
   * same check.
   */
  invariant_version: string;
  /** `critical`, `error`, `warning` or `info`, from the invariant's definition. */
  severity: string;
  message: string;
  remediation: string;
  /** Stable identity for this violation, so the same problem is recognisable across runs. */
  fingerprint: string;
  /**
   * How many times this finding has been observed, across every run. Findings
   * are durable, so a count of eleven means the problem has been seen eleven
   * times rather than that eleven problems exist.
   */
  occurrence_count: number;
  first_seen_at: string;
  last_seen_at: string;
  /** False when the finding is known but the latest run did not reproduce it. */
  observed_on_this_run: boolean;
  /** `open`, `reopened`, `resolved`, `suppressed` or `accepted_risk`. */
  status: string;
}

export interface SystemHealth {
  healthy: boolean;
  findings: SystemHealthFinding[];
  database_role: string;
  database_role_is_superuser: boolean;
  database_role_bypasses_rls: boolean;
  migration_history_compatible: boolean;
}

export interface ProfileItem {
  workspace_id: string;
  principal_id: string;
  identity_source: string;
  authenticated: boolean;
}

const API_BASE = '/api/v1';

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export interface RequestIdentity {
  workspaceId: string;
  principalId: string;
}

/**
 * The backend never substitutes a default identity: a missing or malformed
 * `x-workspace-id` / `x-principal-id` header is answered with 400. Reading the
 * identity up front lets the console say so instead of firing doomed requests.
 */
export function readRequestIdentity(): RequestIdentity {
  return {
    workspaceId: import.meta.env.VITE_VESTRACE_WORKSPACE_ID ?? '',
    principalId: import.meta.env.VITE_VESTRACE_PRINCIPAL_ID ?? '',
  };
}

export function describeIdentityProblem(identity: RequestIdentity): string | null {
  const invalid: string[] = [];
  if (!UUID_PATTERN.test(identity.workspaceId)) invalid.push('VITE_VESTRACE_WORKSPACE_ID');
  if (!UUID_PATTERN.test(identity.principalId)) invalid.push('VITE_VESTRACE_PRINCIPAL_ID');

  if (invalid.length === 0) return null;

  return `${invalid.join(' and ')} must be set to a workspace/principal UUID at build time. The backend rejects requests without a valid identity.`;
}

async function request<T>(endpoint: string, options?: RequestInit): Promise<T> {
  const identity = readRequestIdentity();

  let response: Response;
  try {
    response = await fetch(`${API_BASE}${endpoint}`, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        'x-workspace-id': identity.workspaceId,
        'x-principal-id': identity.principalId,
        ...options?.headers,
      },
    });
  } catch (reason: unknown) {
    throw new ApiRequestError(0, {
      code: 'network_unreachable',
      message: reason instanceof Error ? reason.message : 'the API could not be reached',
    });
  }

  if (!response.ok) {
    let body: ApiErrorBody;
    try {
      body = (await response.json()) as ApiErrorBody;
    } catch {
      body = {
        code: 'invalid_error_response',
        message: `API request failed with status ${response.status}`,
      };
    }
    throw new ApiRequestError(response.status, body);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

export type KernelReadiness = 'ready' | 'degraded' | 'unreachable';

/**
 * `/health/ready` is ungoverned and needs no identity headers, so the shell can
 * report the real kernel state instead of a hardcoded "online" badge.
 */
export async function getKernelReadiness(): Promise<KernelReadiness> {
  try {
    const response = await fetch('/api/health/ready', { headers: { Accept: 'application/json' } });
    if (response.ok) return 'ready';
    // The kernel itself answers 503 when a dependency is not ready. Any other
    // failure comes from the proxy in front of it, i.e. the kernel is not there.
    return response.status === 503 ? 'degraded' : 'unreachable';
  } catch {
    return 'unreachable';
  }
}

export interface WorkspaceSettings {
  workspace_id: string;
  max_concurrent_runs: number;
  run_budget_cap_micros: number;
  budget_unlimited: boolean;
  log_level: string;
  /** Optimistic-concurrency token; send it back as If-Match when saving. */
  version: number;
}

export interface UpdateWorkspaceSettingsPayload {
  max_concurrent_runs: number;
  run_budget_cap_micros: number;
  log_level: string;
}

export interface CreateAgentPayload {
  name: string;
  description: string;
  system_prompt: string;
}

export interface CreateProviderPayload {
  name: string;
  locality: 'local' | 'remote';
}

export interface CreateModelPayload {
  provider_id: string;
  model_name: string;
  context_window: number;
  input_cost_per_mtoken: number;
  output_cost_per_mtoken: number;
}

export interface CreateWorkflowPayload {
  name: string;
}

export interface CreateEvaluationPayload {
  name: string;
  model_id?: string;
  summary?: string;
}

export const vestraceClient = {
  getSettings: (): Promise<WorkspaceSettings> => request<WorkspaceSettings>('/settings'),
  getSystemHealth: (): Promise<SystemHealth> => request<SystemHealth>('/system/health'),
  updateSettings: (
    payload: UpdateWorkspaceSettingsPayload,
    expectedVersion: number,
  ): Promise<WorkspaceSettings> =>
    request<WorkspaceSettings>('/settings', {
      method: 'PUT',
      headers: { 'If-Match': String(expectedVersion) },
      body: JSON.stringify(payload),
    }),
  getMetricsSummary: (): Promise<MetricsSummary> => request<MetricsSummary>('/metrics/summary'),
  listRuns: (): Promise<RunItem[]> => request<RunItem[]>('/runs'),
  createRun: (payload: CreateRunPayload): Promise<RunItem> =>
    request<RunItem>('/runs', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  approveRun: (runId: string): Promise<{ id: string; status: string }> =>
    request<{ id: string; status: string }>(`/runs/${encodeURIComponent(runId)}/approve`, {
      method: 'POST',
    }),
  listArtifacts: (): Promise<ArtifactItem[]> => request<ArtifactItem[]>('/artifacts'),
  listAgents: (): Promise<AgentItem[]> => request<AgentItem[]>('/agents'),
  createAgent: (payload: CreateAgentPayload): Promise<AgentItem> =>
    request<AgentItem>('/agents', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  listWorkflows: (): Promise<WorkflowItem[]> => request<WorkflowItem[]>('/workflows'),
  createWorkflow: (payload: CreateWorkflowPayload): Promise<{ workflow_id: string }> =>
    request<{ workflow_id: string }>('/workflows', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  listTriggers: (): Promise<TriggerItem[]> => request<TriggerItem[]>('/triggers'),
  listConnections: (): Promise<ConnectionItem[]> => request<ConnectionItem[]>('/connections'),
  listModels: (): Promise<ModelItem[]> => request<ModelItem[]>('/models'),
  createModel: (payload: CreateModelPayload): Promise<ModelItem> =>
    request<ModelItem>('/models', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  listProviders: (): Promise<ProviderItem[]> => request<ProviderItem[]>('/providers'),
  createProvider: (payload: CreateProviderPayload): Promise<ProviderItem> =>
    request<ProviderItem>('/providers', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  listEvaluations: (): Promise<EvaluationItem[]> => request<EvaluationItem[]>('/evaluations'),
  createEvaluation: (payload: CreateEvaluationPayload): Promise<{ evaluation_id: string }> =>
    request<{ evaluation_id: string }>('/evaluations', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  listAuditEvents: (): Promise<AuditEventItem[]> => request<AuditEventItem[]>('/audit'),
  getProfile: (): Promise<ProfileItem> => request<ProfileItem>('/profile'),
};
