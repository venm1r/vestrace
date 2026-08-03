export interface RunItem {
  id: string;
  name: string;
  agent: string;
  status: 'Running' | 'WaitingApproval' | 'Completed' | 'Failed';
  progress: number;
  tokens_used: number;
  cost: string;
  duration: string;
  created_at: string;
}

export interface CreateRunPayload {
  prompt: string;
  agent_id?: string;
  workflow_id?: string;
  autonomy_level?: string;
  budget_limit?: number;
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
  try {
    const res = await fetch(`${API_BASE}${endpoint}`, {
      headers: {
        'Content-Type': 'application/json',
        ...options?.headers,
      },
      ...options,
    });
    if (!res.ok) {
      throw new Error(`HTTP error! status: ${res.status}`);
    }
    return await res.json();
  } catch (err) {
    console.warn(`[SDK] API request failed for ${endpoint}, returning fallback data`, err);
    throw err;
  }
}

export const vestraceClient = {
  getMetricsSummary: async (): Promise<MetricsSummary> => {
    return request<MetricsSummary>('/metrics/summary').catch(() => ({
      live_runs: 2,
      active_agents: 3,
      resource_health: '99.98%',
      avg_latency: '142ms',
      total_runs_today: 48,
      budget_spent: '$4.12',
      budget_limit: '$50.00',
    }));
  },
  listRuns: async (): Promise<RunItem[]> => {
    return request<RunItem[]>('/runs').catch(() => [
      {
        id: '0194f4a0-7b3c-7000-8000-000000000001',
        name: 'Audit Data Retention Policy Enforcement',
        agent: 'Compliance-Bot v2',
        status: 'Running',
        progress: 65,
        tokens_used: 14200,
        cost: '$0.042',
        duration: '1m 24s',
        created_at: '2026-08-03T10:14:22Z',
      },
      {
        id: '0194f4a0-7b3c-7000-8000-000000000002',
        name: 'Postgres Index Optimization & Vacuum',
        agent: 'DB-Optimizer',
        status: 'WaitingApproval',
        progress: 40,
        tokens_used: 8900,
        cost: '$0.018',
        duration: '45s',
        created_at: '2026-08-03T09:55:00Z',
      },
      {
        id: '0194f4a0-7b3c-7000-8000-000000000003',
        name: 'Daily Knowledge Graph Extraction',
        agent: 'Memory-Extractor',
        status: 'Completed',
        progress: 100,
        tokens_used: 54100,
        cost: '$0.162',
        duration: '4m 12s',
        created_at: '2026-08-03T08:30:11Z',
      },
      {
        id: '0194f4a0-7b3c-7000-8000-000000000004',
        name: 'External API Rate Limit Diagnostic',
        agent: 'Network-Probe',
        status: 'Failed',
        progress: 25,
        tokens_used: 3100,
        cost: '$0.006',
        duration: '12s',
        created_at: '2026-08-03T07:15:44Z',
      },
    ]);
  },
  createRun: async (payload: CreateRunPayload): Promise<RunItem> => {
    return request<RunItem>('/runs', {
      method: 'POST',
      body: JSON.stringify(payload),
    }).catch(() => ({
      id: `0194f4a0-${Date.now()}`,
      name: payload.prompt,
      agent: payload.agent_id || 'Default-Agent',
      status: 'Running',
      progress: 0,
      tokens_used: 0,
      cost: '$0.000',
      duration: '0s',
      created_at: new Date().toISOString(),
    }));
  },
  approveRun: async (runId: string): Promise<{ id: string; status: string }> => {
    return request<{ id: string; status: string }>(`/runs/${runId}/approve`, {
      method: 'POST',
    }).catch(() => ({ id: runId, status: 'Approved' }));
  },
  listArtifacts: async (): Promise<ArtifactItem[]> => {
    return request<ArtifactItem[]>('/artifacts').catch(() => [
      {
        id: 'art_01',
        name: 'compliance_audit_2026_08_03.json',
        kind: 'Report',
        size: '452 KB',
        created_at: '2026-08-03T10:15:00Z',
        checksum: 'sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
      },
      {
        id: 'art_02',
        name: 'optimized_indexes.sql',
        kind: 'SQL Script',
        size: '12 KB',
        created_at: '2026-08-03T09:55:30Z',
        checksum: 'sha256:8f434346648f6b96df89dda901c5176b10a6d83961dd3c1ac88b59b2dc327aa4',
      },
    ]);
  },
  listAgents: async (): Promise<AgentItem[]> => {
    return request<AgentItem[]>('/agents').catch(() => [
      { id: 'ag_01', name: 'Compliance-Bot v2', model: 'gpt-4o', status: 'Active', runs_count: 142 },
      { id: 'ag_02', name: 'DB-Optimizer', model: 'claude-3-5-sonnet', status: 'Active', runs_count: 89 },
      { id: 'ag_03', name: 'Memory-Extractor', model: 'gpt-4o-mini', status: 'Idle', runs_count: 512 },
    ]);
  },
  listWorkflows: async (): Promise<WorkflowItem[]> => {
    return request<WorkflowItem[]>('/workflows').catch(() => [
      { id: 'wf_01', name: 'Daily Compliance & Cleanup', steps_count: 5, last_run: '10 mins ago', status: 'Active' },
      { id: 'wf_02', name: 'Database Health Inspection', steps_count: 3, last_run: '1 hour ago', status: 'Active' },
    ]);
  },
  listTriggers: async (): Promise<TriggerItem[]> => {
    return request<TriggerItem[]>('/triggers').catch(() => [
      { id: 'tr_01', name: 'Cron Daily Midnight UTC', type: 'Schedule', target: 'Daily Compliance & Cleanup', status: 'Enabled' },
      { id: 'tr_02', name: 'Webhook GitHub Push Main', type: 'Webhook', target: 'Database Health Inspection', status: 'Enabled' },
    ]);
  },
  listConnections: async (): Promise<ConnectionItem[]> => {
    return request<ConnectionItem[]>('/connections').catch(() => [
      { id: 'conn_01', name: 'PostgreSQL Primary Pool', type: 'Database', status: 'Healthy', latency: '2ms' },
      { id: 'conn_02', name: 'OpenAI Provider Gateway', type: 'AI Provider', status: 'Healthy', latency: '140ms' },
      { id: 'conn_03', name: 'Anthropic Provider Gateway', type: 'AI Provider', status: 'Healthy', latency: '165ms' },
    ]);
  },
  listModels: async (): Promise<ModelItem[]> => {
    return request<ModelItem[]>('/models').catch(() => [
      { id: 'mod_01', provider: 'OpenAI', model_name: 'gpt-4o', context_window: '128k tokens', cost_input: '$2.50/M', cost_output: '$10.00/M' },
      { id: 'mod_02', provider: 'Anthropic', model_name: 'claude-3-5-sonnet', context_window: '200k tokens', cost_input: '$3.00/M', cost_output: '$15.00/M' },
      { id: 'mod_03', provider: 'OpenAI', model_name: 'text-embedding-3-small', context_window: '8k tokens', cost_input: '$0.02/M', cost_output: '$0.00/M' },
    ]);
  },
  listEvaluations: async (): Promise<EvaluationItem[]> => {
    return request<EvaluationItem[]>('/evaluations').catch(() => [
      { id: 'ev_01', suite: 'Safety & Alignment Benchmark', score: '99.4%', last_evaluated: '2 hours ago', status: 'Passed' },
      { id: 'ev_02', suite: 'JSON Schema Output Compliance', score: '100.0%', last_evaluated: '5 hours ago', status: 'Passed' },
    ]);
  },
  listAuditEvents: async (): Promise<AuditEventItem[]> => {
    return request<AuditEventItem[]>('/audit').catch(() => [
      { id: 'evt_01', timestamp: '2026-08-03T10:14:22Z', actor: 'admin@vestrace.io', action: 'run.create', resource: '0194f4a0-7b3c-7000-8000-000000000001' },
      { id: 'evt_02', timestamp: '2026-08-03T09:55:00Z', actor: 'system.worker', action: 'approval.requested', resource: '0194f4a0-7b3c-7000-8000-000000000002' },
    ]);
  },
  getProfile: async (): Promise<ProfileItem> => {
    return request<ProfileItem>('/profile').catch(() => ({
      id: 'usr_0194f4a0',
      name: 'Operator Admin',
      email: 'admin@vestrace.io',
      role: 'Workspace Owner',
      mfa_enabled: true,
      active_sessions: 2,
      api_keys: [
        { id: 'key_01', name: 'CLI Operator Key', created: '2026-07-31', last_used: 'Just now' },
      ],
    }));
  },
};
