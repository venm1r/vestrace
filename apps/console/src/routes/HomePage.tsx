import React, { useState } from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';

export const HomePage: React.FC = () => {
  const { data: metrics, error, loading } = useApiResource(vestraceClient.getMetricsSummary);
  const [title, setTitle] = useState('');
  const [creating, setCreating] = useState(false);
  const [mutationError, setMutationError] = useState<string | null>(null);

  const handleCreate = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!title.trim()) return;

    setCreating(true);
    setMutationError(null);
    try {
      const run = await vestraceClient.createRun({ title });
      alert(`Run record created: ${run.id}`);
      setTitle('');
    } catch (reason: unknown) {
      setMutationError(reason instanceof Error ? reason.message : 'Run creation failed');
    } finally {
      setCreating(false);
    }
  };

  if (error) {
    return (
      <div role="alert" style={{ padding: '24px' }}>
        Backend data is unavailable: {error}
      </div>
    );
  }

  if (loading || !metrics) {
    return <div style={{ padding: '40px', color: 'var(--color-on-surface-variant)' }}>Loading P0 status...</div>;
  }

  const cards = [
    ['Live Runs', String(metrics.live_runs)],
    ['Active Agents', String(metrics.active_agents)],
    ['Average Latency', metrics.avg_latency],
    ['Budget', `${metrics.budget_spent} / ${metrics.budget_limit}`],
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      <div>
        <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '24px', fontWeight: 700, color: '#F3F6F9' }}>
          Vestrace P0 Foundation
        </h1>
        <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
          Persisted run records are available. Agent execution, metrics, approvals, and orchestration remain outside P0.
        </p>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: '16px' }}>
        {cards.map(([label, value]) => (
          <div
            key={label}
            style={{
              background: 'var(--color-surface-container-low)',
              border: '1px solid var(--color-outline)',
              borderRadius: '8px',
              padding: '16px',
            }}
          >
            <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)' }}>{label}</div>
            <div style={{ fontFamily: 'var(--font-display)', fontSize: '28px', fontWeight: 700, marginTop: '8px', color: '#F3F6F9' }}>
              {value}
            </div>
          </div>
        ))}
      </div>

      <div
        style={{
          background: 'var(--color-surface-container)',
          border: '1px solid var(--color-primary)',
          borderRadius: '12px',
          padding: '24px',
        }}
      >
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '20px', fontWeight: 600, color: '#F3F6F9' }}>
          Create persisted run record
        </h2>
        <p style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', marginTop: '6px' }}>
          This creates metadata only. It does not execute an agent or workflow.
        </p>

        {mutationError && (
          <div role="alert" style={{ padding: '12px 0', color: 'var(--color-error)' }}>
            Run creation failed: {mutationError}
          </div>
        )}

        <form onSubmit={handleCreate} style={{ display: 'flex', gap: '12px', marginTop: '16px', flexWrap: 'wrap' }}>
          <input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Run title"
            style={{
              flex: 1,
              minWidth: '240px',
              background: 'var(--color-surface-container-lowest)',
              border: '1px solid var(--color-outline)',
              borderRadius: '8px',
              color: '#F3F6F9',
              padding: '10px 14px',
              fontSize: '14px',
            }}
          />
          <button
            type="submit"
            disabled={creating || !title.trim()}
            style={{
              background: 'var(--color-primary)',
              color: '#ffffff',
              border: 'none',
              borderRadius: '8px',
              padding: '10px 20px',
              fontSize: '14px',
              fontWeight: 600,
              cursor: creating ? 'wait' : 'pointer',
            }}
          >
            {creating ? 'Creating...' : 'Create Run'}
          </button>
        </form>
      </div>
    </div>
  );
};
