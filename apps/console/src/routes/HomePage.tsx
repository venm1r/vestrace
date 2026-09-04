import React, { useState } from 'react';
import { Link } from 'react-router-dom';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import {
  ActionButton,
  NoticeBanner,
  PageShell,
  Panel,
  ResourceState,
  describeError,
  useNotice,
} from '../shell/PageState';

export const HomePage: React.FC = () => {
  const { data: metrics, error, loading, reload } = useApiResource(vestraceClient.getMetricsSummary);
  const [title, setTitle] = useState('');
  const [creating, setCreating] = useState(false);
  const [createdRunId, setCreatedRunId] = useState<string | null>(null);
  const { notice, notify, dismiss } = useNotice();

  const handleCreate = async (event: React.FormEvent) => {
    event.preventDefault();
    const trimmed = title.trim();
    if (!trimmed || creating) return;

    setCreatedRunId(null);
    setCreating(true);
    try {
      const run = await vestraceClient.createRun({ title: trimmed });
      setCreatedRunId(run.id);
      setTitle('');
      reload();
      notify('success', `Run record ${run.id} was persisted.`);
    } catch (reason: unknown) {
      const described = describeError(reason, 'run creation');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setCreating(false);
    }
  };

  const metricCards: Array<[string, string]> = metrics
    ? [
        ['Live runs', String(metrics.live_runs)],
        ['Runs today', String(metrics.runs_today)],
        ['Registered agents', String(metrics.registered_agents)],
        ['Registered models', String(metrics.registered_models)],
        [
          'Run budget cap (microunits)',
          metrics.run_budget_cap_micros === null
            ? 'Not set'
            : String(metrics.run_budget_cap_micros),
        ],
      ]
    : [];

  return (
    <PageShell>
      <NoticeBanner notice={notice} onDismiss={dismiss} />

      <Panel style={{ borderColor: 'var(--color-primary)', padding: 'var(--space-xl)' }}>
        <div style={{ maxWidth: '760px', borderLeft: '2px solid var(--brand-cyan)', paddingLeft: 'var(--space-md)' }}>
          <div className="type-label" style={{ color: 'var(--brand-cyan)' }}>Vestrace workspace</div>
          <h1 className="type-h1" style={{ color: 'var(--brand-white)', margin: 'var(--space-xs) 0 0' }}>
            Create a run record
          </h1>
          <p className="type-body" style={{ color: 'var(--text-secondary)', margin: 'var(--space-sm) 0 0' }}>
            Create a named run record, then open it from the Runs workspace.
          </p>
        </div>

        <form onSubmit={handleCreate} style={{ display: 'flex', flexWrap: 'wrap', gap: 'var(--space-sm)', marginTop: 'var(--space-lg)' }}>
          <label htmlFor="run-title" style={{ position: 'absolute', left: '-9999px' }}>
            Run title
          </label>
          <input
            id="run-title"
            className="field-control"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Name this run"
            maxLength={200}
            aria-describedby="run-create-description"
            style={{ flex: '1 1 260px', minWidth: 0 }}
          />
          <ActionButton type="submit" disabled={creating || !title.trim()} icon="add">
            {creating ? 'Creating...' : 'Create run'}
          </ActionButton>
        </form>
        <p id="run-create-description" className="type-body-sm" style={{ color: 'var(--text-secondary)', margin: 'var(--space-sm) 0 0' }}>
          This creates a run record; it does not execute an agent or workflow.
        </p>

        <div className="control-row" style={{ marginTop: 'var(--space-md)' }}>
          <Link to="/runs" className="button-secondary" style={{ textDecoration: 'none' }}>
            Open Runs workspace
          </Link>
          {createdRunId && (
            <Link to={`/runs/${encodeURIComponent(createdRunId)}`} className="type-code" style={{ overflowWrap: 'anywhere' }}>
              Open created run: {createdRunId}
            </Link>
          )}
        </div>
      </Panel>

      <section aria-labelledby="workspace-metrics-heading">
        <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', gap: 'var(--space-md)', marginBottom: 'var(--space-sm)' }}>
          <h2 id="workspace-metrics-heading" className="type-h2" style={{ color: 'var(--brand-white)', margin: 0 }}>
            Workspace metrics
          </h2>
          <span className="type-label" style={{ color: 'var(--text-secondary)' }}>Returned by the workspace API</span>
        </div>
        <Panel>
          <ResourceState
            loading={loading}
            error={error}
            isEmpty={!metrics}
            resourceName="workspace metrics"
            emptyMessage="Workspace metrics are unavailable."
            onRetry={reload}
          />
          {metrics && (
            <dl className="metric-grid" style={{ margin: 0 }}>
              {metricCards.map(([label, value]) => (
                <div key={label} style={{ minWidth: 0, padding: 'var(--space-md)', border: '1px solid var(--color-outline-variant)', background: 'var(--color-surface-container-lowest)' }}>
                  <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>{label}</dt>
                  <dd className="type-h1" style={{ color: 'var(--brand-white)', margin: 'var(--space-xs) 0 0', overflowWrap: 'anywhere' }}>
                    {value}
                  </dd>
                </div>
              ))}
            </dl>
          )}
        </Panel>
      </section>
    </PageShell>
  );
};
