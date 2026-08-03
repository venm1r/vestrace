import React, { useEffect, useState } from 'react';
import { vestraceClient, EvaluationItem } from '../sdk/client';

export const EvaluationsPage: React.FC = () => {
  const [evaluations, setEvaluations] = useState<EvaluationItem[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    vestraceClient.listEvaluations().then(setEvaluations).finally(() => setLoading(false));
  }, []);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '24px', fontWeight: 700, color: '#F3F6F9' }}>
            Evaluations & Safety Benchmarks
          </h1>
          <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
            Automated test suites, safety alignment, schema compliance, and performance metrics.
          </p>
        </div>

        <button
          onClick={() => alert('Triggering automated evaluation suite run...')}
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
          <span className="material-symbols-outlined">insights</span> Run Eval Suite
        </button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
        {loading ? (
          <div style={{ padding: '40px', color: 'var(--color-on-surface-variant)' }}>Loading evaluation suites...</div>
        ) : (
          evaluations.map((ev) => (
            <div
              key={ev.id}
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
                <span className="material-symbols-outlined" style={{ fontSize: '28px', color: 'var(--color-success)' }}>
                  verified
                </span>
                <div>
                  <h3 style={{ fontFamily: 'var(--font-display)', fontSize: '16px', fontWeight: 600, color: '#F3F6F9' }}>
                    {ev.suite}
                  </h3>
                  <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', marginTop: '2px' }}>
                    Last Evaluated: {ev.last_evaluated}
                  </div>
                </div>
              </div>

              <div style={{ display: 'flex', alignItems: 'center', gap: '20px' }}>
                <div style={{ textAlign: 'right' }}>
                  <div style={{ fontFamily: 'var(--font-display)', fontSize: '20px', fontWeight: 700, color: '#00e676' }}>
                    {ev.score}
                  </div>
                  <span style={{ fontSize: '11px', color: 'var(--color-on-surface-variant)', textTransform: 'uppercase' }}>
                    Accuracy Score
                  </span>
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
                  {ev.status}
                </span>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
