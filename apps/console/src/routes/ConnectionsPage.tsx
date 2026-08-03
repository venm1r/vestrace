import React from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';

export const ConnectionsPage: React.FC = () => {
  const { data: connections, error, loading } = useApiResource(vestraceClient.listConnections);

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
            External Connections
          </h1>
          <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
            Credential brokers, PostgreSQL pools, vector databases, and LLM provider gateways.
          </p>
        </div>

        <button
          onClick={() => alert('Connection creation is not implemented in the P0 foundation')}
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
          <span className="material-symbols-outlined">hub</span> Add Connection
        </button>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))', gap: '16px' }}>
        {loading ? (
          <div style={{ padding: '40px', color: 'var(--color-on-surface-variant)' }}>Loading connections...</div>
        ) : (
          (connections ?? []).map((connection) => (
            <div
              key={connection.id}
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
                    hub
                  </span>
                  <div>
                    <h3 style={{ fontFamily: 'var(--font-display)', fontSize: '18px', fontWeight: 600, color: '#F3F6F9' }}>
                      {connection.name}
                    </h3>
                    <span style={{ fontSize: '12px', color: 'var(--color-on-surface-variant)' }}>Type: {connection.type}</span>
                  </div>
                </div>

                <span
                  style={{
                    padding: '2px 8px',
                    borderRadius: '4px',
                    fontSize: '12px',
                    fontWeight: 600,
                    textTransform: 'uppercase',
                    background: 'rgba(0, 230, 118, 0.15)',
                    color: '#00e676',
                  }}
                >
                  {connection.status}
                </span>
              </div>

              <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)' }}>
                Ping Latency: <strong style={{ color: '#F3F6F9', fontFamily: 'var(--font-mono)' }}>{connection.latency}</strong>
              </div>

              <button
                onClick={() => alert(`Connection tests are not implemented for ${connection.name}`)}
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
                Test Connection
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
