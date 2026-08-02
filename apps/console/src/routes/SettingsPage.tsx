import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export const SettingsPage: React.FC = () => {
  const [theme, setTheme] = React.useState<'dark' | 'light' | 'system'>('dark');
  const [density, setDensity] = React.useState<'comfortable' | 'compact'>('comfortable');

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div>
        <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>System Settings</h2>
        <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
          Workspace configuration, RLS tenant isolation boundaries, and interface preferences.
        </p>
      </div>

      {/* Interface Preferences */}
      <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        <h3 style={{ margin: 0, fontSize: '16px', color: 'var(--brand-cyan)' }}>Interface Preferences</h3>
        
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div>
            <strong style={{ display: 'block', fontSize: '14px' }}>Theme Preference</strong>
            <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Switch between Dark, Light, or System OS sync.</span>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
            {(['dark', 'light', 'system'] as const).map((t) => (
              <Button
                key={t}
                variant={theme === t ? 'primary' : 'secondary'}
                onClick={() => setTheme(t)}
                style={{ textTransform: 'capitalize' }}
              >
                {t}
              </Button>
            ))}
          </div>
        </div>

        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 'var(--space-2)' }}>
          <div>
            <strong style={{ display: 'block', fontSize: '14px' }}>Layout Density</strong>
            <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Adjust table and list spacing.</span>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
            {(['comfortable', 'compact'] as const).map((d) => (
              <Button
                key={d}
                variant={density === d ? 'primary' : 'secondary'}
                onClick={() => setDensity(d)}
                style={{ textTransform: 'capitalize' }}
              >
                {d}
              </Button>
            ))}
          </div>
        </div>
      </Surface>

      {/* Security & RLS Policy Status */}
      <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        <h3 style={{ margin: 0, fontSize: '16px', color: 'var(--brand-cyan)' }}>Security & Multi-Tenant Isolation</h3>
        
        <div style={{ fontSize: '13px', display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
          <div><strong>Row-Level Security (RLS):</strong> <span style={{ color: 'var(--semantic-success)', fontWeight: 600 }}>Enforced (vestrace_current_workspace_id())</span></div>
          <div><strong>Envelope Encryption:</strong> <span style={{ color: 'var(--semantic-success)', fontWeight: 600 }}>Active (AES-256-GCM)</span></div>
          <div><strong>Active Workspace ID:</strong> <code style={{ fontSize: '12px', color: 'var(--brand-white)' }}>52000000-0000-0000-0000-000000000001</code></div>
        </div>

        <div style={{ marginTop: 'var(--space-2)' }}>
          <Button variant="danger">Request Security Audit Export</Button>
        </div>
      </Surface>
    </div>
  );
};
