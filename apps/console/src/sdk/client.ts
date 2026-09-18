export interface ApiErrorBody {
  code: string;
  message: string;
}

export class ApiRequestError extends Error {
  public readonly status: number;
  public readonly body: ApiErrorBody;

  constructor(status: number, body: ApiErrorBody) {
    super(body.message);
    this.name = 'ApiRequestError';
    this.status = status;
    this.body = body;
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

/** Legacy create-model response; governed GET does not expose these fields. */
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

/** Safe projection of a governed connection/revision head. */
export interface GovernedConnectionItem {
  id: string;
  revision_id: string | null;
  state: string;
  qualification_state: string | null;
  blockers: string[];
  no_auth_binding_revision_id: string | null;
  execution_guard_id: string | null;
}

export interface GovernedModelItem {
  id: string;
  revision_id: string | null;
  state: string;
  qualification_state: string | null;
  blockers: string[];
}

/** Opaque provider projection backed by a qualified governed tuple. */
export interface GovernedProviderItem {
  id: string;
  state: string;
  blockers: string[];
}

export interface QualificationItem {
  id: string;
  state: string;
  profile_revision: string;
  blockers: string[];
}

export interface CredentialLifecycleItem {
  connection_id: string;
  revision_id: string;
  state: string;
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

export const REQUEST_IDENTITY_HEADER_NAMES = {
  workspace: 'x-workspace-id',
  principal: 'x-principal-id',
} as const;

/**
 * The backend never substitutes a default identity: a missing or malformed
 * `x-workspace-id` / `x-principal-id` header is answered with 400. Reading the
 * identity up front lets the console say so instead of firing doomed requests.
 */
export function readRequestIdentity(): RequestIdentity {
  const env = import.meta.env ?? {};
  return {
    workspaceId: env.VITE_VESTRACE_WORKSPACE_ID ?? '',
    principalId: env.VITE_VESTRACE_PRINCIPAL_ID ?? '',
  };
}

export function buildRequestIdentityHeaders(
  identity: RequestIdentity = readRequestIdentity(),
): Record<string, string> {
  return {
    [REQUEST_IDENTITY_HEADER_NAMES.workspace]: identity.workspaceId,
    [REQUEST_IDENTITY_HEADER_NAMES.principal]: identity.principalId,
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
        ...buildRequestIdentityHeaders(identity),
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

function governedPost<T>(
  endpoint: string,
  payload: unknown | undefined,
  options: GovernedMutationOptions,
): Promise<T> {
  return request<T>(endpoint, {
    method: 'POST',
    headers: { 'idempotency-key': options.requestId },
    ...(payload === undefined ? {} : { body: JSON.stringify(payload) }),
  });
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

export interface CreateModelPayload {
  provider_id: string;
  model_name: string;
  context_window: number;
  input_cost_per_mtoken: number;
  output_cost_per_mtoken: number;
}

/** Mirrors `vestrace_domain::connection::revision::ConnectionKind` (serde snake_case). */
export type ConnectionKind = 'l_m_studio_local' | 'open_ai_chat_completions_v1';

/** Mirrors `vestrace_domain::connection::revision::ConnectionAuthMode` (serde snake_case). */
export type ConnectionAuthMode = 'none' | 'bearer' | 'api_key' | 'x_api_key';

/** Mirrors `vestrace_domain::connection::revision::ConnectionTransportPolicy` (serde tagged, snake_case). */
export type ConnectionTransportPolicy =
  | { kind: 'loopback_only' }
  | { kind: 'remote_https' };

/** Mirrors `api::connections::ConnectionRevisionRequest`. Every id here is minted
 * by the caller (the console), never by the server: the backend does not
 * default, infer or look any of them up. */
export interface CreateConnectionPayload {
  connection_id: string;
  connector_id: string;
  name: string;
  revision_id: string;
  execution_guard_id: string;
  kind: ConnectionKind;
  logical_base_url: string;
  runtime_base_url: string;
  adapter_profile_revision: string;
  transport_policy: ConnectionTransportPolicy;
  auth_mode: ConnectionAuthMode;
  credential_slot_id: string | null;
  expected_head_version: number;
}

/** A later revision on an existing Connection carries every field
 * `CreateConnectionPayload` does; only `expected_head_version` changes meaning
 * (it must equal the Connection's current head, not zero). */
export type CreateConnectionRevisionPayload = CreateConnectionPayload;

/** Mirrors `api::connections::QualificationTargetRequest` (serde internally tagged on `branch`). */
export type QualificationTargetPayload =
  | {
      branch: 'credential';
      revision_id: string;
      slot_id: string;
      activation_guard_id: string;
      expected_slot_version: number;
    }
  | { branch: 'no_auth'; binding_revision_id: string };

/** Mirrors `api::connections::QualificationRequest`. Qualifies one exact
 * Connection revision against one chat and one embedding Model revision. */
export interface QualificationRequestPayload {
  job_id: string;
  target_binding_id: string;
  connection_id: string;
  connection_revision_id: string;
  target: QualificationTargetPayload;
  chat_model_revision_id: string;
  embedding_model_revision_id: string;
}

/** Mirrors `api::connections::CredentialActivationRequest` — the first
 * publication of a credential into an empty slot. The actual secret bytes
 * are not part of this request: `credential_intent_id` names material that
 * must already exist (see Task 5's notes on this gap). */
export interface CreateConnectionCredentialPayload {
  connection_id: string;
  credential_slot_id: string;
  execution_guard_id: string;
  activation_guard_id: string;
  credential_revision_id: string;
  credential_intent_id: string;
  connection_qualification_revision_id: string;
  expected_slot_version: number;
}

/** Mirrors `vestrace_domain::models::binding::ModelKind`. */
export type ModelKind = 'chat' | 'embedding';

/** Mirrors `api::models::ModelRevisionRequest`. Publishes the compatibility
 * Model row and its immutable revision together, bound to one exact
 * Connection revision. */
export interface CreateModelRevisionPayload {
  model_id: string;
  provider_id: string;
  model_name: string;
  context_window: number;
  input_cost_per_mtoken: number;
  output_cost_per_mtoken: number;
  revision_id: string;
  connection_id: string;
  connection_revision_id: string;
  wire_model_id: string;
  kind: ModelKind;
  execution_guard_id: string;
  expected_head_version: number;
}

/** Mirrors `api::models::SetWorkspaceModelDefaultRequest`. */
export interface SetWorkspaceModelDefaultPayload {
  default_id: string;
  model_id: string;
  purpose: string;
  required_capabilities: string[];
  expected_version: number;
}

/** Mirrors `api::models::WorkspaceModelDefaultResponse`. */
export interface WorkspaceModelDefaultItem {
  model_id: string | null;
  purpose: string;
  version: number;
}

export interface GovernedMutationOptions {
  requestId: string;
}

/** Mirrors `api::connections::GovernedMutationResponse`, returned by every
 * governed-mutation route on this backend (Connections, Models, credentials,
 * qualifications) — never a projection of the mutated resource. */
export interface GovernedMutationResponse {
  audit_event_id: string;
  idempotency_key: string | null;
  outbox_message_ids: string[];
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
  listConnections: (): Promise<GovernedConnectionItem[]> =>
    request<GovernedConnectionItem[]>('/connections'),
  createConnection: (
    payload: CreateConnectionPayload,
    options: GovernedMutationOptions,
  ): Promise<GovernedConnectionItem> =>
    governedPost<GovernedConnectionItem>('/connections', payload, options),
  createConnectionRevision: (
    connectionId: string,
    payload: CreateConnectionRevisionPayload,
    options: GovernedMutationOptions,
  ): Promise<GovernedConnectionItem> =>
    governedPost<GovernedConnectionItem>(
      `/connections/${encodeURIComponent(connectionId)}/revisions`,
      payload,
      options,
    ),
  requestConnectionQualification: (
    connectionId: string,
    payload: QualificationRequestPayload,
    options: GovernedMutationOptions,
  ): Promise<QualificationItem> =>
    governedPost<QualificationItem>(
      `/connections/${encodeURIComponent(connectionId)}/qualifications`,
      payload,
      options,
    ),
  createConnectionCredential: (
    connectionId: string,
    payload: CreateConnectionCredentialPayload,
    options: GovernedMutationOptions,
  ): Promise<CredentialLifecycleItem> =>
    governedPost<CredentialLifecycleItem>(
      `/connections/${encodeURIComponent(connectionId)}/credentials`,
      payload,
      options,
    ),
  abandonConnectionCredential: (
    connectionId: string,
    revisionId: string,
    options: GovernedMutationOptions,
  ): Promise<CredentialLifecycleItem> =>
    governedPost<CredentialLifecycleItem>(
      `/connections/${encodeURIComponent(connectionId)}/credentials/${encodeURIComponent(revisionId)}/abandon`,
      undefined,
      options,
    ),
  activateConnectionCredential: (
    connectionId: string,
    revisionId: string,
    options: GovernedMutationOptions,
  ): Promise<CredentialLifecycleItem> =>
    governedPost<CredentialLifecycleItem>(
      `/connections/${encodeURIComponent(connectionId)}/credentials/${encodeURIComponent(revisionId)}/activate`,
      undefined,
      options,
    ),
  revokeConnectionCredential: (
    connectionId: string,
    revisionId: string,
    options: GovernedMutationOptions,
  ): Promise<CredentialLifecycleItem> =>
    governedPost<CredentialLifecycleItem>(
      `/connections/${encodeURIComponent(connectionId)}/credentials/${encodeURIComponent(revisionId)}/revoke`,
      undefined,
      options,
    ),
  listModels: (): Promise<GovernedModelItem[]> => request<GovernedModelItem[]>('/models'),
  createModel: (payload: CreateModelPayload): Promise<ModelItem> =>
    request<ModelItem>('/models', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  createModelRevision: (
    modelId: string,
    payload: CreateModelRevisionPayload,
    options: GovernedMutationOptions,
  ): Promise<GovernedModelItem> =>
    governedPost<GovernedModelItem>(
      `/models/${encodeURIComponent(modelId)}/revisions`,
      payload,
      options,
    ),
  setWorkspaceModelDefault: (
    modelId: string,
    payload: SetWorkspaceModelDefaultPayload,
    options: GovernedMutationOptions,
  ): Promise<GovernedMutationResponse> =>
    governedPost<GovernedMutationResponse>(
      `/models/${encodeURIComponent(modelId)}/default`,
      payload,
      options,
    ),
  getWorkspaceModelDefault: (purpose = 'chat'): Promise<WorkspaceModelDefaultItem> =>
    request<WorkspaceModelDefaultItem>(`/models/default?purpose=${encodeURIComponent(purpose)}`),
  requestModelQualification: (
    modelId: string,
    payload: QualificationRequestPayload,
    options: GovernedMutationOptions,
  ): Promise<QualificationItem> =>
    governedPost<QualificationItem>(
      `/models/${encodeURIComponent(modelId)}/qualifications`,
      payload,
      options,
    ),
  listProviders: (): Promise<GovernedProviderItem[]> =>
    request<GovernedProviderItem[]>('/providers'),
  listEvaluations: (): Promise<EvaluationItem[]> => request<EvaluationItem[]>('/evaluations'),
  createEvaluation: (payload: CreateEvaluationPayload): Promise<{ evaluation_id: string }> =>
    request<{ evaluation_id: string }>('/evaluations', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  listAuditEvents: (): Promise<AuditEventItem[]> => request<AuditEventItem[]>('/audit'),
  getProfile: (): Promise<ProfileItem> => request<ProfileItem>('/profile'),
};
