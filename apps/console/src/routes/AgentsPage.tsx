import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface AgentItem {
  id: string;
  name: string;
  description: string;
  systemPrompt: string;
  status: 'active' | 'draft' | 'deprecated';
  createdAt: string;
}

export const AgentsPage: React.FC = () => {
  const [agents] = React.useState<AgentItem[]>([
    {
      id: 'ag_7f81a4b9',
      name: 'code-reviewer-pro',
      description: 'System-level agent for automated code review, security audits, and pattern enforcement.',
      systemPrompt: 'You are a precise, calm, and evidence-led code reviewer. Analyze ASTs and enforce RLS.',
      status: 'active',
      createdAt: '2026-08-01T09:00:00Z',
    },
    {
      id: 'ag_994a02f8',
      name: 'memory-extractor-bot',
      description: 'Background agent parsing event streams and extracting structured facts.',
      systemPrompt: 'Extract facts, preferences, and constraints from canonical event streams without hallucinating.',
      status: 'active',
      createdAt: '2026-08-01T14:30:00Z',
    },
  ]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>Agents Registry</h2>
          <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
            Registered agent packages, system instructions, capability scopes, and versioned runtime profiles.
          </p>
        </div>
        <Button variant="primary">Register Agent</Button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        {agents.map((ag) => (
          <Surface key={ag.id} level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
                <span style={{ fontFamily: 'monospace', fontWeight: 700, color: 'var(--brand-cyan)' }}>{ag.name}</span>
                <code style={{ fontSize: '12px', color: 'var(--brand-white)' }}>({ag.id})</code>
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
                {ag.status}
              </span>
            </div>

            <p style={{ margin: 0, fontSize: '14px', color: 'var(--text-primary)' }}>{ag.description}</p>

            <div style={{ padding: 'var(--space-2)', backgroundColor: 'var(--bg-level-1)', borderRadius: 'var(--radius-sm)' }}>
              <strong style={{ fontSize: '12px', color: 'var(--brand-muted)', display: 'block', marginBottom: 'var(--space-1)' }}>System Prompt:</strong>
              <code style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>{ag.systemPrompt}</code>
            </div>

            <div style={{ display: 'flex', gap: 'var(--space-2)', marginTop: 'var(--space-1)' }}>
              <Button variant="secondary">Configure Capabilities</Button>
              <Button variant="ghost">View Execution History</Button>
            </div>
          </Surface>
        ))}
      </div>
    </div>
  );
};
