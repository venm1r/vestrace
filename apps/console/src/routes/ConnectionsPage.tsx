import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface ConnectionItem {
  id: string;
  name: string;
  connectorType: string;
  status: 'Active' | 'Revoked' | 'Expired';
  createdAt: string;
}

export const ConnectionsPage: React.FC = () => {
  const [connections] = React.useState<ConnectionItem[]>([
    {
      id: 'conn_77a90b1c',
      name: 'Primary GitHub Enterprise OAuth2',
      connectorType: 'oauth2',
      status: 'Active',
      createdAt: '2026-08-01T10:00:00Z',
    },
    {
      id: 'conn_8812cf90',
      name: 'OpenAI Provider Secret Binding',
      connectorType: 'api_key_envelope',
      status: 'Active',
      createdAt: '2026-08-01T15:45:00Z',
    },
  ]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>Connections & Secrets</h2>
          <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
            OAuth2 connections and encrypted SecretEnvelope credential broker bindings.
          </p>
        </div>
        <Button variant="primary">Add Connection</Button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        {connections.map((conn) => (
          <Surface key={conn.id} level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
                <span style={{ fontFamily: 'monospace', fontWeight: 700, color: 'var(--brand-cyan)' }}>{conn.name}</span>
                <code style={{ fontSize: '12px', color: 'var(--brand-white)' }}>({conn.connectorType})</code>
              </div>
              <span
                style={{
                  padding: '2px 8px',
                  borderRadius: 'var(--radius-round)',
                  fontSize: '11px',
                  fontWeight: 700,
                  textTransform: 'uppercase',
                  backgroundColor: 'var(--semantic-success)',
                  color: '#000',
                }}
              >
                {conn.status}
              </span>
            </div>

            <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>
              Connection ID: <code style={{ color: 'var(--brand-white)' }}>{conn.id}</code>
            </div>

            <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
              <Button variant="secondary">Rotate Credentials</Button>
              <Button variant="danger">Revoke Access</Button>
            </div>
          </Surface>
        ))}
      </div>
    </div>
  );
};
