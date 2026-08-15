import React, { useEffect, useState } from 'react';
import {
  UpdateWorkspaceSettingsPayload,
  WorkspaceSettings,
  vestraceClient,
} from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import {
  ActionButton,
  NoticeBanner,
  PageHeader,
  PageShell,
  Panel,
  ResourceState,
  StatusMessage,
  describeError,
  useNotice,
} from '../shell/PageState';

type TabId = 'kernel' | 'telemetry' | 'environment';

const TABS: Array<{ id: TabId; label: string; icon: string }> = [
  { id: 'kernel', label: 'Kernel Constraints', icon: 'memory' },
  { id: 'telemetry', label: 'Telemetry', icon: 'monitoring' },
  { id: 'environment', label: 'Environment', icon: 'shield' },
];

const LOG_LEVELS = ['error', 'warn', 'info', 'debug', 'trace'] as const;

/** The API stores a budget cap in micro-units; operators think in whole units. */
const MICROS_PER_UNIT = 1_000_000;

interface EditableSettings {
  maxConcurrentRuns: number;
  budgetCapUnits: number;
  logLevel: string;
}

function toEditable(settings: WorkspaceSettings): EditableSettings {
  return {
    maxConcurrentRuns: settings.max_concurrent_runs,
    budgetCapUnits: settings.run_budget_cap_micros / MICROS_PER_UNIT,
    logLevel: settings.log_level,
  };
}

function toPayload(draft: EditableSettings): UpdateWorkspaceSettingsPayload {
  return {
    max_concurrent_runs: draft.maxConcurrentRuns,
    run_budget_cap_micros: Math.round(draft.budgetCapUnits * MICROS_PER_UNIT),
    log_level: draft.logLevel,
  };
}

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

const headingStyle: React.CSSProperties = {
  fontFamily: 'var(--font-display)',
  fontSize: '18px',
  fontWeight: 600,
  color: 'var(--text-primary)',
  marginTop: 0,
  marginBottom: '16px',
};

const Row: React.FC<{
  label: string;
  hint: string;
  control: React.ReactNode;
  htmlFor?: string;
}> = ({ label, hint, control, htmlFor }) => (
  <div
    style={{
      display: 'flex',
      alignItems: 'flex-start',
      justifyContent: 'space-between',
      gap: '24px',
      flexWrap: 'wrap',
    }}
  >
    <div style={{ minWidth: '260px', flex: 1 }}>
      <label htmlFor={htmlFor} style={{ color: 'var(--text-primary)', fontSize: '14px', fontWeight: 500 }}>
        {label}
      </label>
      <p style={{ color: 'var(--text-secondary)', fontSize: '13px', margin: '4px 0 0' }}>{hint}</p>
    </div>
    <div>{control}</div>
  </div>
);

/**
 * A value the runtime reports about its environment. It is shown, never edited:
 * these are determined by the database and process configuration, so a control
 * here would be a control that changes nothing.
 */
const ObservedRow: React.FC<{ label: string; value: string; hint: string }> = ({
  label,
  value,
  hint,
}) => (
  <div
    style={{
      display: 'flex',
      alignItems: 'flex-start',
      justifyContent: 'space-between',
      gap: '24px',
      flexWrap: 'wrap',
    }}
  >
    <div style={{ minWidth: '260px', flex: 1 }}>
      <span style={{ color: 'var(--text-primary)', fontSize: '14px', fontWeight: 500 }}>{label}</span>
      <p style={{ color: 'var(--text-secondary)', fontSize: '13px', margin: '4px 0 0' }}>{hint}</p>
    </div>
    <span
      style={{
        color: 'var(--text-secondary)',
        fontFamily: 'var(--font-mono, monospace)',
        fontSize: '13px',
        padding: '8px 12px',
      }}
    >
      {value}
    </span>
  </div>
);

export const SettingsPage: React.FC = () => {
  const [activeTab, setActiveTab] = useState<TabId>('kernel');
  const { data, error, loading, reload } = useApiResource(vestraceClient.getSettings);
  const { data: health } = useApiResource(vestraceClient.getSystemHealth);
  const [persisted, setPersisted] = useState<WorkspaceSettings | null>(null);
  const [draft, setDraft] = useState<EditableSettings | null>(null);
  const [saving, setSaving] = useState(false);
  const { notice, notify, dismiss } = useNotice();

  useEffect(() => {
    if (data) {
      setPersisted(data);
      setDraft(toEditable(data));
    }
  }, [data]);

  const update = <K extends keyof EditableSettings>(key: K, value: EditableSettings[K]) =>
    setDraft((current) => (current ? { ...current, [key]: value } : current));

  const dirty =
    persisted !== null &&
    draft !== null &&
    (draft.maxConcurrentRuns !== persisted.max_concurrent_runs ||
      Math.round(draft.budgetCapUnits * MICROS_PER_UNIT) !== persisted.run_budget_cap_micros ||
      draft.logLevel !== persisted.log_level);

  const save = async () => {
    if (!persisted || !draft || saving) return;
    setSaving(true);
    try {
      const updated = await vestraceClient.updateSettings(toPayload(draft), persisted.version);
      setPersisted(updated);
      setDraft(toEditable(updated));
      notify('success', `Settings saved as revision ${updated.version}.`);
    } catch (reason: unknown) {
      const described = describeError(reason, 'settings');
      notify('error', `${described.title}: ${described.detail}`);
      // A conflict means someone else advanced the revision; re-read so the
      // operator edits against the current one rather than retrying blindly.
      reload();
    } finally {
      setSaving(false);
    }
  };

  const discard = () => {
    if (persisted) setDraft(toEditable(persisted));
  };

  return (
    <PageShell>
      <PageHeader
        title="Kernel Settings & Policy Rules"
        description="Execution constraints and telemetry for this workspace, plus the environment facts the runtime reports."
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      {/* Settings always resolve to a value: an unconfigured workspace reads as
          defaults, so this surface is never legitimately empty. */}
      <ResourceState
        resourceName="settings"
        loading={loading}
        error={error}
        isEmpty={false}
        emptyMessage=""
        onRetry={reload}
      />

      {draft && persisted && (
        <>
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
                  <span
                    className="material-symbols-outlined"
                    aria-hidden="true"
                    style={{ fontSize: '18px' }}
                  >
                    {tab.icon}
                  </span>
                  {tab.label}
                </button>
              );
            })}
          </div>

          {activeTab === 'kernel' && (
            <div
              role="tabpanel"
              id="settings-panel-kernel"
              aria-labelledby="settings-tab-kernel"
              style={cardStyle}
            >
              <h2 style={headingStyle}>Execution limits</h2>
              <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
                <Row
                  htmlFor="max-concurrent-runs"
                  label="Max concurrent agent runs"
                  hint="Upper bound on runs the workspace may execute at once. Between 1 and 10000."
                  control={
                    <input
                      id="max-concurrent-runs"
                      type="number"
                      min={1}
                      max={10000}
                      value={draft.maxConcurrentRuns}
                      onChange={(event) =>
                        update('maxConcurrentRuns', Number(event.target.value))
                      }
                      style={{ ...fieldStyle, width: '120px' }}
                    />
                  }
                />
                <Row
                  htmlFor="budget-cap"
                  label="Run budget ceiling"
                  hint="Per-run spend ceiling in whole currency units. Zero means no cap is expressed."
                  control={
                    <input
                      id="budget-cap"
                      type="number"
                      min={0}
                      step="0.01"
                      value={draft.budgetCapUnits}
                      onChange={(event) => update('budgetCapUnits', Number(event.target.value))}
                      style={{ ...fieldStyle, width: '120px' }}
                    />
                  }
                />
              </div>
            </div>
          )}

          {activeTab === 'telemetry' && (
            <div
              role="tabpanel"
              id="settings-panel-telemetry"
              aria-labelledby="settings-tab-telemetry"
              style={cardStyle}
            >
              <h2 style={headingStyle}>Telemetry</h2>
              <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
                <Row
                  htmlFor="log-level"
                  label="Workspace log level"
                  hint="Stored with the workspace. The running process also has its own filter, shown under Environment."
                  control={
                    <select
                      id="log-level"
                      value={draft.logLevel}
                      onChange={(event) => update('logLevel', event.target.value)}
                      style={{ ...fieldStyle, width: '140px' }}
                    >
                      {LOG_LEVELS.map((level) => (
                        <option key={level} value={level}>
                          {level.toUpperCase()}
                        </option>
                      ))}
                    </select>
                  }
                />
              </div>
            </div>
          )}

          {activeTab === 'environment' && (
            <div
              role="tabpanel"
              id="settings-panel-environment"
              aria-labelledby="settings-tab-environment"
              style={cardStyle}
            >
              <h2 style={headingStyle}>Environment</h2>
              <Panel>
                <StatusMessage
                  tone="info"
                  title="Reported by the runtime, not configurable here"
                  detail="These are properties of the database and the running process. They are shown so an operator can confirm them, and are deliberately not presented as controls."
                />
              </Panel>
              <div style={{ display: 'flex', flexDirection: 'column', gap: '16px', marginTop: '16px' }}>
                <ObservedRow
                  label="Database role"
                  value={health ? health.database_role : '—'}
                  hint="The role the runtime connects as, reported by the database itself."
                />
                <ObservedRow
                  label="Role bypasses row-level security"
                  value={
                    health ? (health.database_role_bypasses_rls ? 'yes' : 'no') : '—'
                  }
                  hint="Must be no. A role that bypasses RLS can read across every workspace."
                />
                <ObservedRow
                  label="Role is superuser"
                  value={health ? (health.database_role_is_superuser ? 'yes' : 'no') : '—'}
                  hint="Must be no. The runtime role is deliberately unprivileged."
                />
                <ObservedRow
                  label="Migration history"
                  value={
                    health ? (health.migration_history_compatible ? 'compatible' : 'incompatible') : '—'
                  }
                  hint="Whether the applied migrations match the ones this build expects."
                />
                <ObservedRow
                  label="Diagnostic checks"
                  value={health ? (health.healthy ? 'all passed' : `${health.findings.length} finding(s)`) : '—'}
                  hint="Result of the same checks the doctor command runs."
                />
                <ObservedRow
                  label="Operator authentication"
                  value="not implemented"
                  hint="This build has no authentication or MFA. Identity comes from request headers."
                />
              </div>
            </div>
          )}

          <Panel>
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: '16px',
                flexWrap: 'wrap',
              }}
            >
              <span style={{ color: 'var(--text-secondary)', fontSize: '13px' }}>
                Saved revision {persisted.version}
                {dirty ? ' · unsaved changes' : ''}
              </span>
              <div style={{ display: 'flex', gap: '8px' }}>
                <ActionButton variant="quiet" icon="undo" onClick={discard} disabled={!dirty || saving}>
                  Discard
                </ActionButton>
                <ActionButton icon="save" onClick={save} disabled={!dirty || saving}>
                  {saving ? 'Saving…' : 'Save Settings'}
                </ActionButton>
              </div>
            </div>
          </Panel>
        </>
      )}
    </PageShell>
  );
};
