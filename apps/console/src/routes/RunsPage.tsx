import React, { useEffect, useState } from 'react';
import { RunItem, vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';

export const RunsPage: React.FC = () => {
  const { data, error, loading } = useApiResource(vestraceClient.listRuns);
  const [runs, setRuns] = useState<RunItem[]>([]);
  const [creating, setCreating] = useState(false);
  const [mutationError, setMutationError] = useState<string | null>(null);

  useEffect(() => {
    if (data) setRuns(data);
  }, [data]);

  const createRun = async () => {
    setCreating(true);
    setMutationError(null);
    try {
      const newRun = await vestraceClient.createRun({
        title: 'Manual Operator Execution Trigger',
      });
      setRuns((current) => [newRun, ...current]);
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

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: '16px' }}>
        <div>
          <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '24px', fontWeight: 700, color: '#F3F6F9' }}>
            Run Records
          </h1>
          <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
            Persisted run records available in the P0 foundation.
          </p>
        </div>

        <button
          onClick={createRun}
          disabled={creating}
          style={{
            background: 'var(--color-primary)',
            color: '#ffffff',
            border: 'none',
            borderRadius: '8px',
            padding: '10px 18px',
            fontSize: '14px',
            fontWeight: 600,
            cursor: creating ? 'wait' : 'pointer',
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
          }}
        >
          <span className="material-symbols-outlined">add</span>
          {creating ? 'Creating...' : 'Create Run'}
        </button>
      </div>

      {mutationError && (
        <div role="alert" style={{ padding: '12px 16px', color: 'var(--color-error)' }}>
          Run creation failed: {mutationError}
        </div>
      )}

      <div
        style={{
          background: 'var(--color-surface-container-low)',
          border: '1px solid var(--color-outline)',
          borderRadius: '12px',
          overflow: 'hidden',
        }}
      >
        {loading ? (
          <div style={{ padding: '40px', textAlign: 'center', color: 'var(--color-on-surface-variant)' }}>
            Loading persisted runs...
          </div>
        ) : runs.length === 0 ? (
          <div style={{ padding: '40px', textAlign: 'center', color: 'var(--color-on-surface-variant)' }}>
            No run records exist in this workspace.
          </div>
        ) : (
          <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '14px' }}>
            <thead>
              <tr style={{ background: 'var(--color-surface-container)', borderBottom: '1px solid var(--color-outline)', color: 'var(--color-on-surface-variant)', fontSize: '12px', textTransform: 'uppercase' }}>
                <th style={{ padding: '12px 24px' }}>Title & ID</th>
                <th style={{ padding: '12px 16px' }}>Status</th>
                <th style={{ padding: '12px 16px' }}>Version</th>
                <th style={{ padding: '12px 16px' }}>Created</th>
                <th style={{ padding: '12px 24px' }}>Updated</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((run) => (
                <tr key={run.id} style={{ borderBottom: '1px solid var(--color-surface-container-high)' }}>
                  <td style={{ padding: '16px 24px' }}>
                    <div style={{ fontWeight: 600, color: '#F3F6F9' }}>{run.title}</div>
                    <div style={{ fontFamily: 'var(--font-mono)', fontSize: '12px', color: 'var(--color-on-surface-variant)', marginTop: '2px' }}>
                      {run.id}
                    </div>
                  </td>
                  <td style={{ padding: '16px' }}>
                    <span
                      style={{
                        padding: '4px 10px',
                        borderRadius: '4px',
                        fontSize: '12px',
                        fontWeight: 600,
                        textTransform: 'uppercase',
                        background: 'var(--color-surface-container-high)',
                        color: 'var(--color-tertiary)',
                      }}
                    >
                      {run.status}
                    </span>
                  </td>
                  <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>{run.version}</td>
                  <td style={{ padding: '16px' }}>{new Date(run.created_at).toLocaleString()}</td>
                  <td style={{ padding: '16px 24px' }}>{new Date(run.updated_at).toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
};
