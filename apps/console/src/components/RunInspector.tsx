import React from 'react';
import type { MetricsSummary, RunItem } from '../sdk/client';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import { getRunStatusPresentation } from '../routes/runWorkspaceModel';
import { ResourceState } from '../shell/PageState';

function formatTimestamp(value: string): string {
  const timestamp = new Date(value);
  return Number.isNaN(timestamp.getTime()) ? value : timestamp.toLocaleString();
}

const MetricRows: React.FC<{ metrics: MetricsSummary }> = ({ metrics }) => {
  const values: Array<[string, string]> = [
    ['Live runs', String(metrics.live_runs)],
    ['Runs today', String(metrics.runs_today)],
    ['Registered agents', String(metrics.registered_agents)],
    ['Registered models', String(metrics.registered_models)],
    ['Run budget cap (microunits)', metrics.run_budget_cap_micros === null ? 'Not set' : String(metrics.run_budget_cap_micros)],
  ];

  return (
    <dl style={{ display: 'grid', gap: '10px', margin: 0, padding: '12px' }}>
      {values.map(([label, value]) => (
        <div key={label}>
          <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>{label}</dt>
          <dd style={{ margin: '3px 0 0' }}>{value}</dd>
        </div>
      ))}
    </dl>
  );
};

export const RunInspector: React.FC<{ run: RunItem }> = ({ run }) => {
  const { data: metrics, error, loading, reload } = useApiResource(vestraceClient.getMetricsSummary);
  const status = getRunStatusPresentation(run.status);

  return (
    <div className="run-inspector-stack">
      <aside className="run-support-rail" aria-label="Selected run details">
        <div className="pane-heading">Run details</div>
        <dl style={{ display: 'grid', gap: '10px', margin: 0, padding: '12px' }}>
          <div>
            <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>Run ID</dt>
            <dd className="type-code" style={{ margin: '3px 0 0', overflowWrap: 'anywhere' }}>{run.id}</dd>
          </div>
          <div>
            <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>Status</dt>
            <dd style={{ margin: '3px 0 0' }}><span className={`status-chip status-chip--${status.tone}`}>{status.label}</span></dd>
          </div>
          <div>
            <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>Version</dt>
            <dd style={{ margin: '3px 0 0' }}>{run.version}</dd>
          </div>
          <div>
            <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>Created</dt>
            <dd style={{ margin: '3px 0 0' }}><time dateTime={run.created_at}>{formatTimestamp(run.created_at)}</time></dd>
          </div>
          <div>
            <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>Updated</dt>
            <dd style={{ margin: '3px 0 0' }}><time dateTime={run.updated_at}>{formatTimestamp(run.updated_at)}</time></dd>
          </div>
        </dl>
      </aside>

      <aside className="run-metrics-rail" aria-label="Workspace metrics">
        <div className="pane-heading">Workspace metrics</div>
        <ResourceState
          loading={loading}
          error={error}
          isEmpty={!metrics}
          resourceName="workspace metrics"
          emptyMessage="Workspace metrics are unavailable."
          onRetry={reload}
        />
        {metrics && <MetricRows metrics={metrics} />}
      </aside>
    </div>
  );
};
