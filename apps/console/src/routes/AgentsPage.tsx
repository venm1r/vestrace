import React from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';

export const AgentsPage: React.FC = () => {
  const { data: agents, error, loading } = useApiResource(vestraceClient.listAgents);

  if (error) {
    return (
      <div role="alert" style={{ padding: '24px' }}>
        Backend data is unavailable: {error}
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '24px', fontWeight: 700, color: '#F3F6F9' }}>
            Agent Fleet Dashboard
          </h1>
          <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
            Registered autonomous agents, model bindings, and execution statistics.
          </p>
        </div>

        <button
          onClick={() => alert('Agent registration is not implemented in the P0 foundation')}
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
          <span className="material-symbols-outlined">smart_toy</span> Register Agent
        </button>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))', gap: '16px' }}>
        {loading ? (
          <div style={{ padding: '40px', color: 'var(--color-on-surface-variant)' }}>Loading agent fleet...</div>
        ) : (
          (agents ?? []).map((agent) => (
            <div
              key={agent.id}
              style={{
                background: 'var(--color-surface-container-low)',
                border: '1px solid var(--color-outline)',
                borderRadius: '12px',
                padding: '20px',
                display: 'flex',
                flexDirection: 'column',
                gap: '16px',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                  <span className="material-symbols-outlined" style={{ fontSize: '32px', color: 'var(--color-tertiary)' }}>
                    smart_toy
                  </span>
                  <div>
                    <h3 style={{ fontFamily: 'var(--font-display)', fontSize: '18px', fontWeight: 600, color: '#F3F6F9' }}>
                      {agent.name}
                    </h3>
                    <span style={{ fontSize: '12px', color: 'var(--color-on-surface-variant)' }}>ID: {agent.id}</span>
                  </div>
                </div>

                <span
                  style={{
                    padding: '2px 8px',
                    borderRadius: '4px',
                    fontSize: '12px',
                    fontWeight: 600,
                    textTransform: 'uppercase',
                    background: agent.status === 'Active' ? 'rgba(0, 230, 118, 0.15)' : 'rgba(196, 198, 205, 0.15)',
                    color: agent.status === 'Active' ? '#00e676' : '#c4c6cd',
                  }}
                >
                  {agent.status}
                </span>
              </div>

              <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', display: 'flex', flexDirection: 'column', gap: '6px' }}>
                <div>Bound Model: <strong style={{ color: '#F3F6F9' }}>{agent.model}</strong></div>
                <div>Total Lifetime Runs: <strong style={{ color: '#F3F6F9' }}>{agent.runs_count}</strong></div>
              </div>

              <button
                onClick={() => alert(`Agent configuration is not implemented for ${agent.name}`)}
                style={{
                  background: 'var(--color-surface-container-high)',
                  color: '#F3F6F9',
                  border: '1px solid var(--color-outline)',
                  borderRadius: '6px',
                  padding: '8px 14px',
                  fontSize: '13px',
                  cursor: 'pointer',
                  fontWeight: 600,
                }}
              >
                Configure Agent
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
