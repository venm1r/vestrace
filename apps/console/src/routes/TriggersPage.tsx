import React, { useEffect, useState } from 'react';
import { vestraceClient, TriggerItem } from '../sdk/client';

export const TriggersPage: React.FC = () => {
  const [triggers, setTriggers] = useState<TriggerItem[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    vestraceClient.listTriggers().then(setTriggers).finally(() => setLoading(false));
  }, []);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '24px', fontWeight: 700, color: '#F3F6F9' }}>
            Event Triggers & Schedules
          </h1>
          <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
            Configure webhooks, cron schedules, and external event listeners.
          </p>
        </div>

        <button
          onClick={() => alert('New event trigger creation dialog...')}
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
          <span className="material-symbols-outlined">bolt</span> Add Trigger
        </button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
        {loading ? (
          <div style={{ padding: '40px', color: 'var(--color-on-surface-variant)' }}>Loading triggers...</div>
        ) : (
          triggers.map((tr) => (
            <div
              key={tr.id}
              style={{
                background: 'var(--color-surface-container-low)',
                border: '1px solid var(--color-outline)',
                borderRadius: '8px',
                padding: '16px 24px',
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                <span className="material-symbols-outlined" style={{ fontSize: '28px', color: 'var(--color-warning)' }}>
                  bolt
                </span>
                <div>
                  <h3 style={{ fontFamily: 'var(--font-display)', fontSize: '16px', fontWeight: 600, color: '#F3F6F9' }}>
                    {tr.name}
                  </h3>
                  <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', marginTop: '2px', display: 'flex', gap: '16px' }}>
                    <span>Type: <strong>{tr.type}</strong></span>
                    <span>Target: <strong style={{ color: 'var(--color-tertiary)' }}>{tr.target}</strong></span>
                  </div>
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
                {tr.status}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
