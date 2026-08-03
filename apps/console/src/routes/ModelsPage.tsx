import React, { useEffect, useState } from 'react';
import { vestraceClient, ModelItem } from '../sdk/client';

export const ModelsPage: React.FC = () => {
  const [models, setModels] = useState<ModelItem[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    vestraceClient.listModels().then(setModels).finally(() => setLoading(false));
  }, []);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '24px', fontWeight: 700, color: '#F3F6F9' }}>
            AI Models & Providers Registry
          </h1>
          <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
            Model profiles, context windows, token pricing profiles, and embedding models.
          </p>
        </div>

        <button
          onClick={() => alert('Register model profile dialog...')}
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
          <span className="material-symbols-outlined">extension</span> Register Model
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
            Loading model profiles...
          </div>
        ) : (
          <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '14px' }}>
            <thead>
              <tr style={{ background: 'var(--color-surface-container)', borderBottom: '1px solid var(--color-outline)', color: 'var(--color-on-surface-variant)', fontSize: '12px', textTransform: 'uppercase' }}>
                <th style={{ padding: '12px 24px' }}>Model Name</th>
                <th style={{ padding: '12px 16px' }}>Provider</th>
                <th style={{ padding: '12px 16px' }}>Context Window</th>
                <th style={{ padding: '12px 16px' }}>Input Cost</th>
                <th style={{ padding: '12px 16px' }}>Output Cost</th>
              </tr>
            </thead>
            <tbody>
              {models.map((m) => (
                <tr key={m.id} style={{ borderBottom: '1px solid var(--color-surface-container-high)' }}>
                  <td style={{ padding: '16px 24px' }}>
                    <div style={{ fontWeight: 600, color: '#F3F6F9', display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <span className="material-symbols-outlined" style={{ color: 'var(--color-tertiary)' }}>extension</span>
                      {m.model_name}
                    </div>
                  </td>
                  <td style={{ padding: '16px' }}>{m.provider}</td>
                  <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>{m.context_window}</td>
                  <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>{m.cost_input}</td>
                  <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>{m.cost_output}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
};
