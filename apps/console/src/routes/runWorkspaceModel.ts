import type { RunItem, RunStatus } from '../sdk/client';

export type RunGroup = 'active' | 'waiting' | 'finished' | 'other';
export type RunFilter = 'all' | 'active' | 'waiting' | 'finished';
export type RunStatusPresentation = {
  label: string;
  tone: 'neutral' | 'active' | 'warning' | 'success' | 'danger';
};

const ACTIVE_STATUSES = new Set<RunStatus>(['created', 'preparing', 'running']);
const WAITING_STATUSES = new Set<RunStatus>([
  'waiting_for_input',
  'waiting_for_approval',
  'waiting_for_dependency',
  'paused',
  'paused_policy_changed',
]);
const FINISHED_STATUSES = new Set<RunStatus>([
  'succeeded',
  'succeeded_with_warnings',
  'partial',
  'failed',
  'cancelled',
  'expired',
]);

const STATUS_PRESENTATIONS: Record<RunStatus, RunStatusPresentation> = {
  created: { label: 'Created', tone: 'neutral' },
  preparing: { label: 'Preparing', tone: 'active' },
  running: { label: 'Running', tone: 'active' },
  waiting_for_input: { label: 'Waiting for input', tone: 'warning' },
  waiting_for_approval: { label: 'Waiting for approval', tone: 'warning' },
  waiting_for_dependency: { label: 'Waiting for dependency', tone: 'warning' },
  paused: { label: 'Paused', tone: 'warning' },
  paused_policy_changed: { label: 'Paused: policy changed', tone: 'warning' },
  succeeded: { label: 'Succeeded', tone: 'success' },
  succeeded_with_warnings: { label: 'Succeeded with warnings', tone: 'warning' },
  partial: { label: 'Partial', tone: 'warning' },
  failed: { label: 'Failed', tone: 'danger' },
  cancelled: { label: 'Cancelled', tone: 'neutral' },
  expired: { label: 'Expired', tone: 'neutral' },
};

const UNKNOWN_STATUS_PRESENTATION: RunStatusPresentation = {
  label: 'Unknown',
  tone: 'neutral',
};

export function getRunGroup(status: RunStatus | string): RunGroup {
  if (ACTIVE_STATUSES.has(status as RunStatus)) return 'active';
  if (WAITING_STATUSES.has(status as RunStatus)) return 'waiting';
  if (FINISHED_STATUSES.has(status as RunStatus)) return 'finished';
  return 'other';
}

export function filterRuns(
  runs: readonly RunItem[],
  query: string,
  filter: RunFilter,
): RunItem[] {
  const normalizedQuery = query.trim().toLowerCase();

  return runs.filter((run) => {
    if (filter !== 'all' && getRunGroup(run.status) !== filter) return false;
    if (normalizedQuery === '') return true;

    return (
      run.title.toLowerCase().includes(normalizedQuery) || run.id.toLowerCase().includes(normalizedQuery)
    );
  });
}

export function groupRuns(runs: readonly RunItem[]): Record<RunGroup, RunItem[]> {
  const groups: Record<RunGroup, RunItem[]> = {
    active: [],
    waiting: [],
    finished: [],
    other: [],
  };

  for (const run of runs) {
    groups[getRunGroup(run.status)].push(run);
  }

  return groups;
}

export function resolveSelectedRun(
  runs: readonly RunItem[],
  requestedId?: string,
): RunItem | null {
  return runs.find(({ id }) => id === requestedId) ?? runs[0] ?? null;
}

export function getRunStatusPresentation(status: RunStatus | string): RunStatusPresentation {
  return STATUS_PRESENTATIONS[status as RunStatus] ?? UNKNOWN_STATUS_PRESENTATION;
}
