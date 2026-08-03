import React from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';

export const ModelsPage: React.FC = () => {
  const { data: models, error, loading } = useApiResource(vestraceClient.listModels);

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
            AI Models & Providers Registry
          </h1>
          <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
            Model profiles, context windows, token pricing profiles, and embedding models.
          </p>
        </div>

        <button
          onClick={() => alert('Model registration is not implemented in the P0 foundation')}
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
              {(models ?? []).map((model) => (
                <tr key={model.id} style={{ borderBottom: '1px solid var(--color-surface-container-high)' }}>
                  <td style={{ padding: '16px 24px' }}>
                    <div style={{ fontWeight: 600, color: '#F3F6F9', display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <span className="material-symbols-outlined" style={{ color: 'var(--color-tertiary)' }}>extension</span>
                      {model.model_name}
                    </div>
                  </td>
                  <td style={{ padding: '16px' }}>{model.provider}</td>
                  <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>{model.context_window}</td>
                  <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>{model.cost_input}</td>
                  <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>{model.cost_output}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
};
