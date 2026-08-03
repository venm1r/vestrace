import React, { useEffect, useState } from 'react';
import { vestraceClient, AuditEventItem } from '../sdk/client';

export const AuditPage: React.FC = () => {
  const [events, setEvents] = useState<AuditEventItem[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    vestraceClient.listAuditEvents().then(setEvents).finally(() => setLoading(false));
  }, []);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '24px', fontWeight: 700, color: '#F3F6F9' }}>
            Audit Log & Trace Registry
          </h1>
          <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
            Immutable security event stream, capability authorization logs, and RLS enforcement history.
          </p>
        </div>

        <button
          onClick={() => alert('Exporting audit log verification proof...')}
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
          <span className="material-symbols-outlined">shield</span> Export Audit Trail
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
        {loading ? (
          <div style={{ padding: '40px', textAlign: 'center', color: 'var(--color-on-surface-variant)' }}>
            Loading audit stream...
          </div>
        ) : (
          <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '14px' }}>
            <thead>
              <tr style={{ background: 'var(--color-surface-container)', borderBottom: '1px solid var(--color-outline)', color: 'var(--color-on-surface-variant)', fontSize: '12px', textTransform: 'uppercase' }}>
                <th style={{ padding: '12px 24px' }}>Timestamp</th>
                <th style={{ padding: '12px 16px' }}>Actor</th>
                <th style={{ padding: '12px 16px' }}>Action</th>
                <th style={{ padding: '12px 24px' }}>Target Resource</th>
              </tr>
            </thead>
            <tbody>
              {events.map((evt) => (
                <tr key={evt.id} style={{ borderBottom: '1px solid var(--color-surface-container-high)' }}>
                  <td style={{ padding: '16px 24px', fontFamily: 'var(--font-mono)', fontSize: '13px', color: 'var(--color-on-surface-variant)' }}>
                    {evt.timestamp}
                  </td>
                  <td style={{ padding: '16px', fontWeight: 600, color: '#F3F6F9' }}>{evt.actor}</td>
                  <td style={{ padding: '16px' }}>
                    <span style={{ padding: '4px 8px', background: 'var(--color-surface-container-high)', borderRadius: '4px', fontSize: '12px', fontFamily: 'var(--font-mono)', color: 'var(--color-tertiary)' }}>
                      {evt.action}
                    </span>
                  </td>
                  <td style={{ padding: '16px 24px', fontFamily: 'var(--font-mono)', fontSize: '13px' }}>{evt.resource}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
};
