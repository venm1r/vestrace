import React, { useState } from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import {
  ActionButton,
  NoticeBanner,
  PageHeader,
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
  const { notice, notify, dismiss } = useNotice();

  const handleCreate = async (event: React.FormEvent) => {
    event.preventDefault();
    const trimmed = title.trim();
    if (!trimmed || creating) return;

    setCreating(true);
    try {
      const run = await vestraceClient.createRun({ title: trimmed });
      notify('success', `Run record ${run.id} was persisted.`);
      setTitle('');
    } catch (reason: unknown) {
      const described = describeError(reason, 'run creation');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setCreating(false);
    }
  };

  const cards: Array<[string, string]> = metrics
    ? [
        ['Live Runs', String(metrics.live_runs)],
        ['Runs Today', String(metrics.runs_today)],
        ['Registered Agents', String(metrics.registered_agents)],
        [
          'Run Budget Ceiling',
          metrics.run_budget_cap_micros === null
            ? 'no cap set'
            : (metrics.run_budget_cap_micros / 1_000_000).toFixed(2),
        ],
      ]
    : [];

  return (
    <PageShell>
      <PageHeader
        title="Vestrace Execution Kernel"
        description="Autonomous agent execution, run coordination, metrics observation, and policy approvals."
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      <Panel>
        <ResourceState
          loading={loading}
          error={error}
          isEmpty={!metrics}
          resourceName="the metrics summary"
          emptyMessage="No metrics were returned for this workspace."
          onRetry={reload}
        />
        {metrics && (
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))',
              gap: '1px',
              background: 'var(--color-outline)',
            }}
          >
            {cards.map(([label, value]) => (
              <div key={label} style={{ background: 'var(--color-surface-container-low)', padding: '16px 24px' }}>
                <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>{label}</div>
                <div
                  style={{
                    fontFamily: 'var(--font-display)',
                    fontSize: '28px',
                    fontWeight: 700,
                    marginTop: '8px',
                    color: 'var(--text-primary)',
                  }}
                >
                  {value}
                </div>
              </div>
            ))}
          </div>
        )}
      </Panel>

      <Panel style={{ border: '1px solid var(--color-primary)', padding: '24px' }}>
        <h2
          style={{
            fontFamily: 'var(--font-display)',
            fontSize: '20px',
            fontWeight: 600,
            color: 'var(--text-primary)',
            margin: 0,
          }}
        >
          Create persisted run record
        </h2>
        <p style={{ fontSize: '13px', color: 'var(--text-secondary)', marginTop: '6px' }}>
          This creates metadata only. It does not execute an agent or workflow.
        </p>

        <form onSubmit={handleCreate} style={{ display: 'flex', gap: '12px', marginTop: '16px', flexWrap: 'wrap' }}>
          <label htmlFor="run-title" style={{ position: 'absolute', left: '-9999px' }}>
            Run title
          </label>
          <input
            id="run-title"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Run title"
            maxLength={200}
            style={{
              flex: 1,
              minWidth: '240px',
              background: 'var(--color-surface-container-lowest)',
              border: '1px solid var(--color-outline)',
              borderRadius: '8px',
              color: 'var(--text-primary)',
              padding: '10px 14px',
              fontSize: '14px',
            }}
          />
          <ActionButton type="submit" disabled={creating || !title.trim()} icon="add">
            {creating ? 'Creating...' : 'Create Run'}
          </ActionButton>
        </form>
      </Panel>
    </PageShell>
  );
};
