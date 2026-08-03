import React, { useState } from 'react';

export const SettingsPage: React.FC = () => {
  const [activeTab, setActiveTab] = useState<'kernel' | 'security' | 'database' | 'telemetry'>('kernel');
  const [rlsEnforced, setRlsEnforced] = useState(true);
  const [mfaEnforced, setMfaEnforced] = useState(true);
  const [logLevel, setLogLevel] = useState('INFO');
  const [dbPoolSize, setDbPoolSize] = useState(25);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      {/* Settings Header */}
      <div>
        <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '24px', fontWeight: 700, color: '#F3F6F9' }}>
          Kernel Settings & Policy Rules
        </h1>
        <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
          Configure Row-Level Security, envelope encryption, execution constraints, and telemetry.
        </p>
      </div>

      {/* Settings Sub-Navigation Tabs */}
      <div
        style={{
          display: 'flex',
          gap: '8px',
          borderBottom: '1px solid var(--color-outline)',
          paddingBottom: '12px',
        }}
      >
        {[
          { id: 'kernel', label: 'Kernel Constraints', icon: 'memory' },
          { id: 'security', label: 'Security & RLS', icon: 'shield' },
          { id: 'database', label: 'Database & Pools', icon: 'database' },
          { id: 'telemetry', label: 'Telemetry & Logs', icon: 'monitoring' },
        ].map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id as any)}
            style={{
              background: activeTab === tab.id ? 'var(--color-surface-container-high)' : 'transparent',
              color: activeTab === tab.id ? '#ffffff' : 'var(--color-on-surface-variant)',
              border: 'none',
              borderRadius: '6px',
              padding: '8px 16px',
              fontSize: '14px',
              fontWeight: activeTab === tab.id ? 600 : 400,
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              gap: '8px',
            }}
          >
            <span className="material-symbols-outlined" style={{ fontSize: '18px' }}>
              {tab.icon}
            </span>
            {tab.label}
          </button>
        ))}
      </div>

      {/* Tab Content Panels */}
      {activeTab === 'kernel' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
          <div style={{ background: 'var(--color-surface-container-low)', border: '1px solid var(--color-outline)', borderRadius: '12px', padding: '24px' }}>
            <h3 style={{ fontFamily: 'var(--font-display)', fontSize: '18px', fontWeight: 600, color: '#F3F6F9', marginBottom: '16px' }}>
              Execution Limits & Autonomy Controls
            </h3>
            
            <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <div style={{ fontWeight: 600, color: '#F3F6F9' }}>Max Concurrent Agent Runs</div>
                  <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)' }}>Limit concurrent worker task execution threads per workspace</div>
                </div>
                <input
                  type="number"
                  defaultValue={10}
                  style={{
                    background: 'var(--color-surface-container-lowest)',
                    border: '1px solid var(--color-outline)',
                    borderRadius: '6px',
                    color: '#F3F6F9',
                    padding: '8px 12px',
                    width: '80px',
                    textAlign: 'center',
                  }}
                />
              </div>

              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <div style={{ fontWeight: 600, color: '#F3F6F9' }}>Default Task Budget Cap</div>
                  <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)' }}>Maximum dollar budget allowed for unapproved automated runs</div>
                </div>
                <input
                  type="text"
                  defaultValue="$10.00"
                  style={{
                    background: 'var(--color-surface-container-lowest)',
                    border: '1px solid var(--color-outline)',
                    borderRadius: '6px',
                    color: '#F3F6F9',
                    padding: '8px 12px',
                    width: '100px',
                    textAlign: 'center',
                  }}
                />
              </div>
            </div>
          </div>
        </div>
      )}

      {activeTab === 'security' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
          <div style={{ background: 'var(--color-surface-container-low)', border: '1px solid var(--color-outline)', borderRadius: '12px', padding: '24px' }}>
            <h3 style={{ fontFamily: 'var(--font-display)', fontSize: '18px', fontWeight: 600, color: '#F3F6F9', marginBottom: '16px' }}>
              Row-Level Security & Envelope Encryption
            </h3>

            <div style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <div style={{ fontWeight: 600, color: '#F3F6F9' }}>Enforce Strict PostgreSQL RLS</div>
                  <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)' }}>
                    Require session variable `vestrace.workspace_id` on all database operations
                  </div>
                </div>
                <button
                  onClick={() => setRlsEnforced(!rlsEnforced)}
                  style={{
                    background: rlsEnforced ? 'var(--color-success)' : 'var(--color-outline)',
                    color: rlsEnforced ? '#000000' : '#ffffff',
                    border: 'none',
                    borderRadius: '12px',
                    padding: '6px 16px',
                    fontSize: '13px',
                    fontWeight: 700,
                    cursor: 'pointer',
                  }}
                >
                  {rlsEnforced ? 'ENABLED' : 'DISABLED'}
                </button>
              </div>

              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <div style={{ fontWeight: 600, color: '#F3F6F9' }}>Require MFA for Operator Actions</div>
                  <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)' }}>
                    Challenge high-risk policy approvals with multi-factor authentication
                  </div>
                </div>
                <button
                  onClick={() => setMfaEnforced(!mfaEnforced)}
                  style={{
                    background: mfaEnforced ? 'var(--color-success)' : 'var(--color-outline)',
                    color: mfaEnforced ? '#000000' : '#ffffff',
                    border: 'none',
                    borderRadius: '12px',
                    padding: '6px 16px',
                    fontSize: '13px',
                    fontWeight: 700,
                    cursor: 'pointer',
                  }}
                >
                  {mfaEnforced ? 'ENFORCED' : 'OPTIONAL'}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {activeTab === 'database' && (
        <div style={{ background: 'var(--color-surface-container-low)', border: '1px solid var(--color-outline)', borderRadius: '12px', padding: '24px' }}>
          <h3 style={{ fontFamily: 'var(--font-display)', fontSize: '18px', fontWeight: 600, color: '#F3F6F9', marginBottom: '16px' }}>
            PostgreSQL Primary Connection Pool
          </h3>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div>
                <div style={{ fontWeight: 600, color: '#F3F6F9' }}>Max Pool Connections</div>
                <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)' }}>Maximum active connections in sqlx connection pool</div>
              </div>
              <input
                type="number"
                value={dbPoolSize}
                onChange={(e) => setDbPoolSize(Number(e.target.value))}
                style={{
                  background: 'var(--color-surface-container-lowest)',
                  border: '1px solid var(--color-outline)',
                  borderRadius: '6px',
                  color: '#F3F6F9',
                  padding: '8px 12px',
                  width: '80px',
                  textAlign: 'center',
                }}
              />
            </div>
          </div>
        </div>
      )}

      {activeTab === 'telemetry' && (
        <div style={{ background: 'var(--color-surface-container-low)', border: '1px solid var(--color-outline)', borderRadius: '12px', padding: '24px' }}>
          <h3 style={{ fontFamily: 'var(--font-display)', fontSize: '18px', fontWeight: 600, color: '#F3F6F9', marginBottom: '16px' }}>
            Tracing & Tracing Subsystem Level
          </h3>

          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <div>
              <div style={{ fontWeight: 600, color: '#F3F6F9' }}>Tracing Verbosity Level</div>
              <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)' }}>Rust tracing log level filter</div>
            </div>
            <select
              value={logLevel}
              onChange={(e) => setLogLevel(e.target.value)}
              style={{
                background: 'var(--color-surface-container-lowest)',
                border: '1px solid var(--color-outline)',
                borderRadius: '6px',
                color: '#F3F6F9',
                padding: '8px 16px',
                fontSize: '14px',
              }}
            >
              <option value="ERROR">ERROR</option>
              <option value="WARN">WARN</option>
              <option value="INFO">INFO</option>
              <option value="DEBUG">DEBUG</option>
              <option value="TRACE">TRACE</option>
            </select>
          </div>
        </div>
      )}

      {/* Save Settings Action Bar */}
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '12px', marginTop: '12px' }}>
        <button
          onClick={() => alert('Settings changes discarded')}
          style={{
            background: 'transparent',
            border: '1px solid var(--color-outline)',
            color: 'var(--color-on-surface-variant)',
            borderRadius: '8px',
            padding: '10px 20px',
            fontSize: '14px',
            cursor: 'pointer',
          }}
        >
          Cancel
        </button>
        <button
          onClick={() => alert('Kernel settings saved and applied to runtime session')}
          style={{
            background: 'var(--color-primary)',
            color: '#ffffff',
            border: 'none',
            borderRadius: '8px',
            padding: '10px 24px',
            fontSize: '14px',
            fontWeight: 600,
            cursor: 'pointer',
          }}
        >
          Save Settings
        </button>
      </div>
    </div>
  );
};
