import React, { useState } from 'react';
import {
  ActionButton,
  NoticeBanner,
  PageHeader,
  PageShell,
  Panel,
  StatusMessage,
  useNotice,
} from '../shell/PageState';

type TabId = 'kernel' | 'security' | 'database' | 'telemetry';

const TABS: Array<{ id: TabId; label: string; icon: string }> = [
  { id: 'kernel', label: 'Kernel Constraints', icon: 'memory' },
  { id: 'security', label: 'Security & RLS', icon: 'shield' },
  { id: 'database', label: 'Database & Pools', icon: 'database' },
  { id: 'telemetry', label: 'Telemetry & Logs', icon: 'monitoring' },
];

interface KernelSettings {
  maxConcurrentRuns: number;
  budgetCapUsd: number;
  rlsEnforced: boolean;
  mfaEnforced: boolean;
  dbPoolSize: number;
  logLevel: string;
}

const DEFAULT_SETTINGS: KernelSettings = {
  maxConcurrentRuns: 10,
  budgetCapUsd: 10,
  rlsEnforced: true,
  mfaEnforced: true,
  dbPoolSize: 25,
  logLevel: 'INFO',
};

const cardStyle: React.CSSProperties = {
  background: 'var(--color-surface-container-low)',
  border: '1px solid var(--color-outline)',
  borderRadius: '12px',
  padding: '24px',
};

const fieldStyle: React.CSSProperties = {
  background: 'var(--color-surface-container-lowest)',
  border: '1px solid var(--color-outline)',
  borderRadius: '6px',
  color: 'var(--text-primary)',
  padding: '8px 12px',
};

const Row: React.FC<{ label: string; hint: string; control: React.ReactNode; htmlFor?: string }> = ({
  label,
  hint,
  control,
  htmlFor,
}) => (
  <div
    style={{
      display: 'flex',
      justifyContent: 'space-between',
      alignItems: 'center',
      gap: '16px',
      flexWrap: 'wrap',
    }}
  >
    <div style={{ minWidth: '240px', flex: 1 }}>
      <label htmlFor={htmlFor} style={{ fontWeight: 600, color: 'var(--text-primary)', display: 'block' }}>
        {label}
      </label>
      <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>{hint}</div>
    </div>
    {control}
  </div>
);

const Toggle: React.FC<{
  checked: boolean;
  onChange: (next: boolean) => void;
  onLabel: string;
  offLabel: string;
  describes: string;
}> = ({ checked, onChange, onLabel, offLabel, describes }) => (
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={describes}
    onClick={() => onChange(!checked)}
    style={{
      background: checked ? 'var(--color-success)' : 'var(--color-surface-container-high)',
      color: checked ? '#04140a' : 'var(--text-secondary)',
      border: '1px solid var(--color-outline)',
      borderRadius: '12px',
      padding: '6px 16px',
      fontSize: '13px',
      fontWeight: 700,
      cursor: 'pointer',
      minWidth: '110px',
    }}
  >
    {checked ? onLabel : offLabel}
  </button>
);

export const SettingsPage: React.FC = () => {
  const [activeTab, setActiveTab] = useState<TabId>('kernel');
  const [settings, setSettings] = useState<KernelSettings>(DEFAULT_SETTINGS);
  const { notice, notify, dismiss } = useNotice();

  const update = <K extends keyof KernelSettings>(key: K, value: KernelSettings[K]) =>
    setSettings((current) => ({ ...current, [key]: value }));

  const dirty = (Object.keys(DEFAULT_SETTINGS) as Array<keyof KernelSettings>).some(
    (key) => settings[key] !== DEFAULT_SETTINGS[key],
  );

  return (
    <PageShell>
      <PageHeader
        title="Kernel Settings & Policy Rules"
        description="Row-Level Security, envelope encryption, execution constraints, and telemetry."
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      <Panel>
        <StatusMessage
          tone="warning"
          title="Settings are not persisted in the P0 foundation"
          detail="The HTTP adapter exposes no settings endpoint, so edits made here stay in this browser tab and are discarded on reload. The controls show the intended configuration surface."
        />
      </Panel>

      <div
        role="tablist"
        aria-label="Settings sections"
        style={{
          display: 'flex',
          gap: '8px',
          borderBottom: '1px solid var(--color-outline)',
          paddingBottom: '12px',
          flexWrap: 'wrap',
        }}
      >
        {TABS.map((tab) => {
          const selected = activeTab === tab.id;
          return (
            <button
              key={tab.id}
              type="button"
              role="tab"
              id={`settings-tab-${tab.id}`}
              aria-selected={selected}
              aria-controls={`settings-panel-${tab.id}`}
              onClick={() => setActiveTab(tab.id)}
              style={{
                background: selected ? 'var(--color-surface-container-high)' : 'transparent',
                color: selected ? 'var(--text-primary)' : 'var(--text-secondary)',
                border: 'none',
                borderRadius: '6px',
                padding: '8px 16px',
                fontSize: '14px',
                fontWeight: selected ? 600 : 400,
                cursor: 'pointer',
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
              }}
            >
              <span className="material-symbols-outlined" aria-hidden="true" style={{ fontSize: '18px' }}>
                {tab.icon}
              </span>
              {tab.label}
            </button>
          );
        })}
      </div>

      {activeTab === 'kernel' && (
        <div role="tabpanel" id="settings-panel-kernel" aria-labelledby="settings-tab-kernel" style={cardStyle}>
          <h2
            style={{
              fontFamily: 'var(--font-display)',
              fontSize: '18px',
              fontWeight: 600,
              color: 'var(--text-primary)',
              marginTop: 0,
              marginBottom: '16px',
            }}
          >
            Execution limits and autonomy controls
          </h2>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
            <Row
              htmlFor="max-concurrent-runs"
              label="Max concurrent agent runs"
              hint="Limit concurrent worker task execution threads per workspace"
              control={
                <input
                  id="max-concurrent-runs"
                  type="number"
                  min={1}
                  max={1000}
                  value={settings.maxConcurrentRuns}
                  onChange={(event) => update('maxConcurrentRuns', Number(event.target.value))}
                  style={{ ...fieldStyle, width: '96px', textAlign: 'center' }}
                />
              }
            />

            <Row
              htmlFor="budget-cap"
              label="Default task budget cap (USD)"
              hint="Maximum budget allowed for unapproved automated runs"
              control={
                <input
                  id="budget-cap"
                  type="number"
                  min={0}
                  step={0.5}
                  value={settings.budgetCapUsd}
                  onChange={(event) => update('budgetCapUsd', Number(event.target.value))}
                  style={{ ...fieldStyle, width: '110px', textAlign: 'center' }}
                />
              }
            />
          </div>
        </div>
      )}

      {activeTab === 'security' && (
        <div role="tabpanel" id="settings-panel-security" aria-labelledby="settings-tab-security" style={cardStyle}>
          <h2
            style={{
              fontFamily: 'var(--font-display)',
              fontSize: '18px',
              fontWeight: 600,
              color: 'var(--text-primary)',
              marginTop: 0,
              marginBottom: '16px',
            }}
          >
            Row-Level Security and envelope encryption
          </h2>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
            <Row
              label="Enforce strict PostgreSQL RLS"
              hint="Require the vestrace.workspace_id session variable on all database operations"
              control={
                <Toggle
                  checked={settings.rlsEnforced}
                  onChange={(next) => update('rlsEnforced', next)}
                  onLabel="ENABLED"
                  offLabel="DISABLED"
                  describes="Enforce strict PostgreSQL RLS"
                />
              }
            />

            <Row
              label="Require MFA for operator actions"
              hint="Challenge high-risk policy approvals with multi-factor authentication"
              control={
                <Toggle
                  checked={settings.mfaEnforced}
                  onChange={(next) => update('mfaEnforced', next)}
                  onLabel="ENFORCED"
                  offLabel="OPTIONAL"
                  describes="Require MFA for operator actions"
                />
              }
            />
          </div>
        </div>
      )}

      {activeTab === 'database' && (
        <div role="tabpanel" id="settings-panel-database" aria-labelledby="settings-tab-database" style={cardStyle}>
          <h2
            style={{
              fontFamily: 'var(--font-display)',
              fontSize: '18px',
              fontWeight: 600,
              color: 'var(--text-primary)',
              marginTop: 0,
              marginBottom: '16px',
            }}
          >
            PostgreSQL primary connection pool
          </h2>

          <Row
            htmlFor="db-pool-size"
            label="Max pool connections"
            hint="Maximum active connections in the sqlx connection pool"
            control={
              <input
                id="db-pool-size"
                type="number"
                min={1}
                max={500}
                value={settings.dbPoolSize}
                onChange={(event) => update('dbPoolSize', Number(event.target.value))}
                style={{ ...fieldStyle, width: '96px', textAlign: 'center' }}
              />
            }
          />
        </div>
      )}

      {activeTab === 'telemetry' && (
        <div role="tabpanel" id="settings-panel-telemetry" aria-labelledby="settings-tab-telemetry" style={cardStyle}>
          <h2
            style={{
              fontFamily: 'var(--font-display)',
              fontSize: '18px',
              fontWeight: 600,
              color: 'var(--text-primary)',
              marginTop: 0,
              marginBottom: '16px',
            }}
          >
            Tracing subsystem
          </h2>

          <Row
            htmlFor="log-level"
            label="Tracing verbosity level"
            hint="Rust tracing log level filter"
            control={
              <select
                id="log-level"
                value={settings.logLevel}
                onChange={(event) => update('logLevel', event.target.value)}
                style={{ ...fieldStyle, padding: '8px 16px', fontSize: '14px' }}
              >
                {['ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE'].map((level) => (
                  <option key={level} value={level}>
                    {level}
                  </option>
                ))}
              </select>
            }
          />
        </div>
      )}

      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '12px' }}>
        <ActionButton
          variant="quiet"
          disabled={!dirty}
          onClick={() => {
            setSettings(DEFAULT_SETTINGS);
            notify('info', 'Local edits were reverted to the defaults shown by the console.');
          }}
        >
          Reset
        </ActionButton>
        <ActionButton
          disabled
          title="No settings endpoint exists in the P0 foundation"
          onClick={() => undefined}
        >
          Save Settings
        </ActionButton>
      </div>
    </PageShell>
  );
};
