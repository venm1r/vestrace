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

  /** The backend answers 501 for surfaces that are deliberately out of P0 scope. */
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

/*
 * The surfaces below are reserved by the HTTP adapter but answer 501 in the P0
 * foundation. The shapes describe the intended contract; the console renders an
 * explicit "not implemented" state until the endpoints exist.
 */

export interface ArtifactItem {
  id: string;
  name: string;
  kind: string;
  size: string;
  created_at: string;
  checksum: string;
}

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

export interface MetricsSummary {
  live_runs: number;
  active_agents: number;
  resource_health: string;
  avg_latency: string;
  total_runs_today: number;
  budget_spent: string;
  budget_limit: string;
}

export interface ProfileItem {
  id: string;
  name: string;
  email: string;
  role: string;
  mfa_enabled: boolean;
  active_sessions: number;
  api_keys: Array<{ id: string; name: string; created: string; last_used: string }>;
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

export const vestraceClient = {
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
  listWorkflows: (): Promise<WorkflowItem[]> => request<WorkflowItem[]>('/workflows'),
  listTriggers: (): Promise<TriggerItem[]> => request<TriggerItem[]>('/triggers'),
  listConnections: (): Promise<ConnectionItem[]> => request<ConnectionItem[]>('/connections'),
  listModels: (): Promise<ModelItem[]> => request<ModelItem[]>('/models'),
  listProviders: (): Promise<ProviderItem[]> => request<ProviderItem[]>('/providers'),
  listEvaluations: (): Promise<EvaluationItem[]> => request<EvaluationItem[]>('/evaluations'),
  listAuditEvents: (): Promise<AuditEventItem[]> => request<AuditEventItem[]>('/audit'),
  getProfile: (): Promise<ProfileItem> => request<ProfileItem>('/profile'),
};
