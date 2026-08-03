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
}

export type RunStatus =
  | 'created'
  | 'running'
  | 'waiting_for_input'
  | 'waiting_for_approval'
  | 'completed'
  | 'failed'
  | 'cancelled';

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

export interface ArtifactItem {
  id: string;
  name: string;
  kind: string;
  size: string;
  created_at: string;
  checksum: string;
}

export interface AgentItem {
  id: string;
  name: string;
  model: string;
  status: string;
  runs_count: number;
}

export interface WorkflowItem {
  id: string;
  name: string;
  steps_count: number;
  last_run: string;
  status: string;
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

export interface ModelItem {
  id: string;
  provider: string;
  model_name: string;
  context_window: string;
  cost_input: string;
  cost_output: string;
}

export interface EvaluationItem {
  id: string;
  suite: string;
  score: string;
  last_evaluated: string;
  status: string;
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

async function request<T>(endpoint: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${endpoint}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      'x-workspace-id': import.meta.env.VITE_VESTRACE_WORKSPACE_ID ?? '',
      'x-principal-id': import.meta.env.VITE_VESTRACE_PRINCIPAL_ID ?? '',
      ...options?.headers,
    },
  });

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

  return (await response.json()) as T;
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
    request<{ id: string; status: string }>(`/runs/${runId}/approve`, {
      method: 'POST',
    }),
  listArtifacts: (): Promise<ArtifactItem[]> => request<ArtifactItem[]>('/artifacts'),
  listAgents: (): Promise<AgentItem[]> => request<AgentItem[]>('/agents'),
  listWorkflows: (): Promise<WorkflowItem[]> => request<WorkflowItem[]>('/workflows'),
  listTriggers: (): Promise<TriggerItem[]> => request<TriggerItem[]>('/triggers'),
  listConnections: (): Promise<ConnectionItem[]> => request<ConnectionItem[]>('/connections'),
  listModels: (): Promise<ModelItem[]> => request<ModelItem[]>('/models'),
  listEvaluations: (): Promise<EvaluationItem[]> => request<EvaluationItem[]>('/evaluations'),
  listAuditEvents: (): Promise<AuditEventItem[]> => request<AuditEventItem[]>('/audit'),
  getProfile: (): Promise<ProfileItem> => request<ProfileItem>('/profile'),
};
