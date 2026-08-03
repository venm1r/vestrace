import React, { useEffect, useState } from 'react';
import { vestraceClient, RunItem } from '../sdk/client';

export const RunsPage: React.FC = () => {
  const [runs, setRuns] = useState<RunItem[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    vestraceClient
      .listRuns()
      .then(setRuns)
      .finally(() => setLoading(false));
  }, []);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '24px', fontWeight: 700, color: '#F3F6F9' }}>
            Execution History & Agent Runs
          </h1>
          <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
            Authoritative event-sourced runs, step transitions, and execution telemetry.
          </p>
        </div>

        <button
          onClick={async () => {
            const newRun = await vestraceClient.createRun({ prompt: 'Manual Operator Execution Trigger' });
            setRuns((prev) => [newRun, ...prev]);
          }}
          style={{
            background: 'var(--color-primary)',
            color: '#ffffff',
            border: 'none',
            borderRadius: '8px',
            padding: '10px 18px',
            fontSize: '14px',
            fontWeight: 600,
            cursor: 'pointer',
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
          }}
        >
          <span className="material-symbols-outlined">add</span> Create New Run
        </button>
      </div>

      <div
        style={{
          background: 'var(--color-surface-container-low)',
          border: '1px solid var(--color-outline)',
          borderRadius: '12px',
          overflow: 'hidden',
        }}
      >
        <div style={{ padding: '16px 24px', borderBottom: '1px solid var(--color-outline)', display: 'flex', gap: '16px', alignItems: 'center' }}>
          <div style={{ position: 'relative', flex: 1 }}>
            <span className="material-symbols-outlined" style={{ position: 'absolute', left: '12px', top: '10px', color: 'var(--color-on-surface-variant)' }}>
              search
            </span>
            <input
              type="text"
              placeholder="Search runs by ID, agent or task name..."
              style={{
                width: '100%',
                background: 'var(--color-surface-container-lowest)',
                border: '1px solid var(--color-outline)',
                borderRadius: '6px',
                padding: '8px 12px 8px 40px',
                color: '#F3F6F9',
                fontSize: '14px',
                fontFamily: 'var(--font-sans)',
              }}
            />
          </div>

          <select
            style={{
              background: 'var(--color-surface-container-lowest)',
              border: '1px solid var(--color-outline)',
              borderRadius: '6px',
              color: '#F3F6F9',
              padding: '8px 12px',
              fontSize: '14px',
            }}
          >
            <option value="all">All Statuses</option>
            <option value="running">Running</option>
            <option value="waiting">Waiting Approval</option>
            <option value="completed">Completed</option>
            <option value="failed">Failed</option>
          </select>
        </div>

        {loading ? (
          <div style={{ padding: '40px', textAlign: 'center', color: 'var(--color-on-surface-variant)' }}>
            Loading execution history...
          </div>
        ) : (
          <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '14px' }}>
            <thead>
              <tr style={{ background: 'var(--color-surface-container)', borderBottom: '1px solid var(--color-outline)', color: 'var(--color-on-surface-variant)', fontSize: '12px', textTransform: 'uppercase' }}>
                <th style={{ padding: '12px 24px' }}>Run Name & ID</th>
                <th style={{ padding: '12px 16px' }}>Agent</th>
                <th style={{ padding: '12px 16px' }}>Status</th>
                <th style={{ padding: '12px 16px' }}>Tokens</th>
                <th style={{ padding: '12px 16px' }}>Cost</th>
                <th style={{ padding: '12px 16px' }}>Duration</th>
                <th style={{ padding: '12px 24px', textAlign: 'right' }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((r) => (
                <tr key={r.id} style={{ borderBottom: '1px solid var(--color-surface-container-high)' }}>
                  <td style={{ padding: '16px 24px' }}>
                    <div style={{ fontWeight: 600, color: '#F3F6F9' }}>{r.name}</div>
                    <div style={{ fontFamily: 'var(--font-mono)', fontSize: '12px', color: 'var(--color-on-surface-variant)', marginTop: '2px' }}>
                      {r.id}
                    </div>
                  </td>
                  <td style={{ padding: '16px' }}>{r.agent}</td>
                  <td style={{ padding: '16px' }}>
                    <span
                      style={{
                        padding: '4px 10px',
                        borderRadius: '4px',
                        fontSize: '12px',
                        fontWeight: 600,
                        textTransform: 'uppercase',
                        background:
                          r.status === 'Running'
                            ? 'rgba(37, 99, 235, 0.2)'
                            : r.status === 'WaitingApproval'
                            ? 'rgba(255, 184, 0, 0.2)'
                            : r.status === 'Completed'
                            ? 'rgba(0, 230, 118, 0.2)'
                            : 'rgba(255, 77, 79, 0.2)',
                        color:
                          r.status === 'Running'
                            ? '#3b82f6'
                            : r.status === 'WaitingApproval'
                            ? '#ffb800'
                            : r.status === 'Completed'
                            ? '#00e676'
                            : '#ff4d4f',
                      }}
                    >
                      {r.status}
                    </span>
                  </td>
                  <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>{r.tokens_used.toLocaleString()}</td>
                  <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>{r.cost}</td>
                  <td style={{ padding: '16px' }}>{r.duration}</td>
                  <td style={{ padding: '16px 24px', textAlign: 'right' }}>
                    <button
                      onClick={() => alert(`Inspecting trace for run ${r.id}`)}
                      style={{
                        background: 'var(--color-surface-container-high)',
                        color: '#F3F6F9',
                        border: '1px solid var(--color-outline)',
                        borderRadius: '6px',
                        padding: '6px 12px',
                        fontSize: '13px',
                        cursor: 'pointer',
                      }}
                    >
                      Inspect Trace
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
};
