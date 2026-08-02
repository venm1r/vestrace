import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export const SettingsPage: React.FC = () => {
  const [theme, setTheme] = React.useState<'dark' | 'light' | 'system'>('dark');
  const [density, setDensity] = React.useState<'comfortable' | 'compact'>('comfortable');
  const [fontFamily, setFontFamily] = React.useState<'Inter' | 'Geist' | 'Manrope' | 'Space Grotesk'>('Inter');
  const [monoFont, setMonoFont] = React.useState<'JetBrains Mono' | 'IBM Plex Mono'>('JetBrains Mono');
  const [logFormat, setLogFormat] = React.useState<'text' | 'json'>('json');
  const [writePolicy, setWritePolicy] = React.useState<'automatic' | 'assisted' | 'manual'>('automatic');
  const [maxRetries, setMaxRetries] = React.useState<number>(3);
  const [tokenBudget, setTokenBudget] = React.useState<number>(8000);
  const [auditLevel, setAuditLevel] = React.useState<'Minimal' | 'Operational' | 'Reproducible' | 'Forensic'>('Operational');
  const [mcpEnabled, setMcpEnabled] = React.useState<boolean>(true);
  const [a2aGatewayEnabled, setA2aGatewayEnabled] = React.useState<boolean>(true);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div>
        <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>System Settings & Configuration</h2>
        <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
          Comprehensive platform configuration, typography, tenant limits, security bounds, and router policies.
        </p>
      </div>

      {/* 1. Interface & Typography Preferences */}
      <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        <h3 style={{ margin: 0, fontSize: '16px', color: 'var(--brand-cyan)' }}>1. Interface & Typography Preferences</h3>
        
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
            <strong style={{ display: 'block', fontSize: '14px' }}>Primary Sans-Serif Font (design.md Section 8)</strong>
            <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Approved body and UI font family.</span>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
            {(['Inter', 'Geist', 'Manrope', 'Space Grotesk'] as const).map((f) => (
              <Button
                key={f}
                variant={fontFamily === f ? 'primary' : 'secondary'}
                onClick={() => setFontFamily(f)}
              >
                {f}
              </Button>
            ))}
          </div>
        </div>

        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 'var(--space-2)' }}>
          <div>
            <strong style={{ display: 'block', fontSize: '14px' }}>Monospace Code Font</strong>
            <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Used for identifiers, hashes, and technical data.</span>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
            {(['JetBrains Mono', 'IBM Plex Mono'] as const).map((mf) => (
              <Button
                key={mf}
                variant={monoFont === mf ? 'primary' : 'secondary'}
                onClick={() => setMonoFont(mf)}
              >
                {mf}
              </Button>
            ))}
          </div>
        </div>

        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 'var(--space-2)' }}>
          <div>
            <strong style={{ display: 'block', fontSize: '14px' }}>Layout Density</strong>
            <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Adjust table, kanban, and list spacing.</span>
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

      {/* 2. Memory & Ingestion Policy Settings */}
      <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        <h3 style={{ margin: 0, fontSize: '16px', color: 'var(--brand-cyan)' }}>2. Memory Core & Ingestion Policy</h3>
        
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div>
            <strong style={{ display: 'block', fontSize: '14px' }}>Memory Write Policy</strong>
            <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Controls automatic memory candidate activation threshold.</span>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
            {(['automatic', 'assisted', 'manual'] as const).map((p) => (
              <Button
                key={p}
                variant={writePolicy === p ? 'primary' : 'secondary'}
                onClick={() => setWritePolicy(p)}
                style={{ textTransform: 'capitalize' }}
              >
                {p}
              </Button>
            ))}
          </div>
        </div>
      </Surface>

      {/* 3. AI Model Router & Gateway Protocols */}
      <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        <h3 style={{ margin: 0, fontSize: '16px', color: 'var(--brand-cyan)' }}>3. AI Model Router & Gateway Protocols</h3>
        <div style={{ fontSize: '13px', display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
          <div>
            <strong>Default Provider Base URL:</strong>
            <input
              type="text"
              defaultValue="http://localhost:8080/v1"
              style={{
                width: '100%',
                marginTop: 'var(--space-1)',
                padding: 'var(--space-2)',
                backgroundColor: 'var(--bg-level-1)',
                border: '1px solid var(--border-color)',
                borderRadius: 'var(--radius-sm)',
                color: 'var(--brand-white)',
              }}
            />
          </div>

          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <div>
              <strong style={{ display: 'block' }}>Model Context Protocol (MCP) Server</strong>
              <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Enable stdio & HTTP transport server.</span>
            </div>
            <Button variant={mcpEnabled ? 'primary' : 'secondary'} onClick={() => setMcpEnabled(!mcpEnabled)}>
              {mcpEnabled ? 'ENABLED' : 'DISABLED'}
            </Button>
          </div>

          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <div>
              <strong style={{ display: 'block' }}>A2A Interoperability Gateway</strong>
              <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Allow inter-agent RPC communication.</span>
            </div>
            <Button variant={a2aGatewayEnabled ? 'primary' : 'secondary'} onClick={() => setA2aGatewayEnabled(!a2aGatewayEnabled)}>
              {a2aGatewayEnabled ? 'ENABLED' : 'DISABLED'}
            </Button>
          </div>

          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <div>
              <strong style={{ display: 'block' }}>Context Pack Token Budget</strong>
              <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Max tokens allocated per context snapshot.</span>
            </div>
            <input
              type="number"
              value={tokenBudget}
              onChange={(e) => setTokenBudget(Number(e.target.value))}
              style={{
                width: '80px',
                padding: 'var(--space-1) var(--space-2)',
                backgroundColor: 'var(--bg-level-1)',
                border: '1px solid var(--border-color)',
                borderRadius: 'var(--radius-sm)',
                color: 'var(--brand-white)',
              }}
            />
          </div>
        </div>
      </Surface>

      {/* 4. Worker & Retry Policy Settings */}
      <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        <h3 style={{ margin: 0, fontSize: '16px', color: 'var(--brand-cyan)' }}>4. Worker Queue & Execution Leases</h3>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div>
            <strong style={{ display: 'block', fontSize: '14px' }}>Max Task Attempt Retries</strong>
            <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Maximum retry attempts before dead-letter queueing.</span>
          </div>
          <input
            type="number"
            value={maxRetries}
            onChange={(e) => setMaxRetries(Number(e.target.value))}
            style={{
              width: '80px',
              padding: 'var(--space-1) var(--space-2)',
              backgroundColor: 'var(--bg-level-1)',
              border: '1px solid var(--border-color)',
              borderRadius: 'var(--radius-sm)',
              color: 'var(--brand-white)',
            }}
          />
        </div>
      </Surface>

      {/* 5. Security, Audit & Observability */}
      <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        <h3 style={{ margin: 0, fontSize: '16px', color: 'var(--brand-cyan)' }}>5. Security, Audit & Observability Profiles</h3>
        
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div>
            <strong style={{ display: 'block', fontSize: '14px' }}>Audit Capture Profile</strong>
            <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Select audit logging depth.</span>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
            {(['Minimal', 'Operational', 'Reproducible', 'Forensic'] as const).map((l) => (
              <Button
                key={l}
                variant={auditLevel === l ? 'primary' : 'secondary'}
                onClick={() => setAuditLevel(l)}
              >
                {l}
              </Button>
            ))}
          </div>
        </div>

        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 'var(--space-2)' }}>
          <div>
            <strong style={{ display: 'block', fontSize: '14px' }}>Structured Log Format</strong>
            <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>Format application stdout telemetry.</span>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
            {(['text', 'json'] as const).map((f) => (
              <Button
                key={f}
                variant={logFormat === f ? 'primary' : 'secondary'}
                onClick={() => setLogFormat(f)}
                style={{ textTransform: 'uppercase' }}
              >
                {f}
              </Button>
            ))}
          </div>
        </div>

        <div style={{ fontSize: '13px', display: 'flex', flexDirection: 'column', gap: 'var(--space-1)', marginTop: 'var(--space-3)' }}>
          <div><strong>Row-Level Security (RLS):</strong> <span style={{ color: 'var(--semantic-success)', fontWeight: 600 }}>Enforced (vestrace_current_workspace_id())</span></div>
          <div><strong>Envelope Encryption:</strong> <span style={{ color: 'var(--semantic-success)', fontWeight: 600 }}>Active (AES-256-GCM)</span></div>
          <div><strong>Active Workspace ID:</strong> <code style={{ fontSize: '12px', color: 'var(--brand-white)' }}>52000000-0000-0000-0000-000000000001</code></div>
        </div>

        <div style={{ marginTop: 'var(--space-3)' }}>
          <Button variant="danger">Export Full Audit & Key Escrow Manifest</Button>
        </div>
      </Surface>
    </div>
  );
};
