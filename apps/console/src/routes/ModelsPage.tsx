import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface ModelItem {
  id: string;
  name: string;
  provider: string;
  contextWindow: number;
  inputCost: number;
  outputCost: number;
}

export const ModelsPage: React.FC = () => {
  const [models] = React.useState<ModelItem[]>([
    {
      id: 'mdl_44091a2c',
      name: 'gpt-4o',
      provider: 'OpenAI Compatible Provider',
      contextWindow: 128000,
      inputCost: 2.50,
      outputCost: 10.00,
    },
    {
      id: 'mdl_8820c41f',
      name: 'claude-3-5-sonnet',
      provider: 'Anthropic Provider Adapter',
      contextWindow: 200000,
      inputCost: 3.00,
      outputCost: 15.00,
    },
  ]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>Models & Providers</h2>
          <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
            Registered AI provider models, context windows, cost profiles, and fallback policies.
          </p>
        </div>
        <Button variant="primary" onClick={() => alert('Opening AI Model Registration Wizard...')}>
          Register Model
        </Button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        {models.map((mdl) => (
          <Surface key={mdl.id} level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
                <span style={{ fontFamily: 'monospace', fontWeight: 700, color: 'var(--brand-cyan)' }}>{mdl.name}</span>
                <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>({mdl.provider})</span>
              </div>
              <span style={{ fontSize: '12px', color: 'var(--brand-white)', fontWeight: 600 }}>
                {(mdl.contextWindow / 1000).toFixed(0)}k Context
              </span>
            </div>

            <div style={{ fontSize: '13px', color: 'var(--text-secondary)', display: 'flex', gap: 'var(--space-5)' }}>
              <span>Input: <strong>${mdl.inputCost.toFixed(2)} / 1M tokens</strong></span>
              <span>Output: <strong>${mdl.outputCost.toFixed(2)} / 1M tokens</strong></span>
            </div>

            <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
              <Button variant="secondary" onClick={() => alert(`Configuring fallback policy for ${mdl.name}...`)}>
                Configure Fallback Rules
              </Button>
              <Button variant="ghost" onClick={() => alert(`Ping test for ${mdl.provider}: 200 OK (45ms)`)}>
                Test Connection
              </Button>
            </div>
          </Surface>
        ))}
      </div>
    </div>
  );
};
