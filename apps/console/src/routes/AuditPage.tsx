import React from 'react';
import { Surface } from '../design-system/primitives/Surface';

export interface AuditEventItem {
  id: string;
  action: string;
  resourceType: string;
  resourceId: string;
  principalId: string;
  createdAt: string;
}

export const AuditPage: React.FC = () => {
  const [events] = React.useState<AuditEventItem[]>([
    {
      id: 'aud_90f81a2c',
      action: 'capability.check',
      resourceType: 'memory',
      resourceId: 'mem_994a02f8',
      principalId: 'prc_7a18f409',
      createdAt: '2026-08-02T12:00:00Z',
    },
    {
      id: 'aud_1120ab44',
      action: 'system.deploy_schema',
      resourceType: 'database',
      resourceId: 'cluster_prod_01',
      principalId: 'prc_7a18f409',
      createdAt: '2026-08-02T12:05:00Z',
    },
  ]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div>
        <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>Audit Log</h2>
        <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
          Content-free security audit trail, access events, and capability verification logs.
        </p>
      </div>

      <Surface level={2} style={{ padding: 0, overflow: 'hidden' }}>
        <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '13px' }}>
          <thead>
            <tr style={{ backgroundColor: 'var(--bg-level-1)', borderBottom: '1px solid var(--border-color)', color: 'var(--text-secondary)' }}>
              <th style={{ padding: 'var(--space-3)' }}>Action</th>
              <th style={{ padding: 'var(--space-3)' }}>Resource Type</th>
              <th style={{ padding: 'var(--space-3)' }}>Resource ID</th>
              <th style={{ padding: 'var(--space-3)' }}>Principal ID</th>
              <th style={{ padding: 'var(--space-3)' }}>Timestamp</th>
            </tr>
          </thead>
          <tbody>
            {events.map((evt) => (
              <tr key={evt.id} style={{ borderBottom: '1px solid var(--border-color)', color: 'var(--text-primary)' }}>
                <td style={{ padding: 'var(--space-3)', fontFamily: 'monospace', color: 'var(--brand-cyan)' }}>{evt.action}</td>
                <td style={{ padding: 'var(--space-3)' }}>{evt.resourceType}</td>
                <td style={{ padding: 'var(--space-3)', fontFamily: 'monospace' }}>{evt.resourceId}</td>
                <td style={{ padding: 'var(--space-3)', fontFamily: 'monospace' }}>{evt.principalId}</td>
                <td style={{ padding: 'var(--space-3)', color: 'var(--text-secondary)' }}>{evt.createdAt}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Surface>
    </div>
  );
};
