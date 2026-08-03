import React from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';

export const TriggersPage: React.FC = () => {
  const { data: triggers, error, loading } = useApiResource(vestraceClient.listTriggers);

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
            Event Triggers & Schedules
          </h1>
          <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
            Configure webhooks, cron schedules, and external event listeners.
          </p>
        </div>

        <button
          onClick={() => alert('Trigger creation is not implemented in the P0 foundation')}
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
          (triggers ?? []).map((trigger) => (
            <div
              key={trigger.id}
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
                    {trigger.name}
                  </h3>
                  <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', marginTop: '2px', display: 'flex', gap: '16px' }}>
                    <span>Type: <strong>{trigger.type}</strong></span>
                    <span>Target: <strong style={{ color: 'var(--color-tertiary)' }}>{trigger.target}</strong></span>
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
                {trigger.status}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
