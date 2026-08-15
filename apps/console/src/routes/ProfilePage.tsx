import React from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import {
  ActionButton,
  NoticeBanner,
  PageHeader,
  PageShell,
  Panel,
  ResourceState,
  useNotice,
} from '../shell/PageState';

function initialsOf(name: string): string {
  const initials = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? '')
    .join('');
  return initials || '?';
}

export const ProfilePage: React.FC = () => {
  const { data: profile, error, loading, reload } = useApiResource(vestraceClient.getProfile);
  const { notice, notify, dismiss } = useNotice();

  const apiKeys = profile?.api_keys ?? [];

  return (
    <PageShell>
      <PageHeader
        title="Operator Profile"
        description="Identity, session posture, and the API credentials issued to this operator."
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      {(loading || error || !profile) && (
        <Panel>
          <ResourceState
            loading={loading}
            error={error}
            isEmpty={!profile}
            resourceName="the operator profile"
            emptyMessage="No profile was returned for this principal."
            onRetry={reload}
          />
        </Panel>
      )}

      {profile && (
        <>
          <Panel style={{ padding: '24px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '16px', marginBottom: '20px' }}>
              <div
                aria-hidden="true"
                style={{
                  width: '64px',
                  height: '64px',
                  borderRadius: '50%',
                  background: 'var(--color-primary)',
                  color: '#ffffff',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  fontSize: '24px',
                  fontWeight: 700,
                  fontFamily: 'var(--font-display)',
                  flexShrink: 0,
                }}
              >
                {initialsOf(profile.name)}
              </div>
              <div style={{ minWidth: 0 }}>
                <h2
                  style={{
                    fontFamily: 'var(--font-display)',
                    fontSize: '22px',
                    fontWeight: 600,
                    color: 'var(--text-primary)',
                    margin: 0,
                  }}
                >
                  {profile.name}
                </h2>
                <p style={{ fontSize: '14px', color: 'var(--text-secondary)', margin: '4px 0 0 0' }}>
                  {profile.email} • <span style={{ color: 'var(--color-tertiary)' }}>{profile.role}</span>
                </p>
              </div>
            </div>

            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))', gap: '16px' }}>
              <div
                style={{
                  background: 'var(--color-surface-container)',
                  padding: '16px',
                  borderRadius: '8px',
                  border: '1px solid var(--color-surface-container-high)',
                }}
              >
                <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>MFA security status</div>
                <div
                  style={{
                    fontSize: '16px',
                    fontWeight: 600,
                    color: profile.mfa_enabled ? 'var(--color-success)' : 'var(--color-error)',
                    marginTop: '6px',
                    display: 'flex',
                    alignItems: 'center',
                    gap: '6px',
                  }}
                >
                  <span className="material-symbols-outlined" aria-hidden="true">
                    {profile.mfa_enabled ? 'verified' : 'warning'}
                  </span>
                  {profile.mfa_enabled ? 'Enforced and active' : 'Disabled'}
                </div>
              </div>

              <div
                style={{
                  background: 'var(--color-surface-container)',
                  padding: '16px',
                  borderRadius: '8px',
                  border: '1px solid var(--color-surface-container-high)',
                }}
              >
                <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>Active operator sessions</div>
                <div
                  style={{
                    fontSize: '16px',
                    fontWeight: 600,
                    color: 'var(--text-primary)',
                    marginTop: '6px',
                    display: 'flex',
                    alignItems: 'center',
                    gap: '6px',
                  }}
                >
                  <span className="material-symbols-outlined" aria-hidden="true">
                    devices
                  </span>
                  {profile.active_sessions} concurrent
                </div>
              </div>
            </div>
          </Panel>

          <Panel style={{ padding: '24px' }}>
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                gap: '16px',
                marginBottom: '16px',
                flexWrap: 'wrap',
              }}
            >
              <h2
                style={{
                  fontFamily: 'var(--font-display)',
                  fontSize: '18px',
                  fontWeight: 600,
                  color: 'var(--text-primary)',
                  margin: 0,
                }}
              >
                API keys and tokens
              </h2>
              <ActionButton
                icon="add"
                style={{ padding: '8px 16px', fontSize: '13px' }}
                onClick={() => notify('info', 'API key creation is not implemented in this build.')}
              >
                Create API Key
              </ActionButton>
            </div>

            {apiKeys.length === 0 ? (
              <p style={{ color: 'var(--text-secondary)', fontSize: '14px', margin: 0 }}>
                No API keys are issued to this operator.
              </p>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
                {apiKeys.map((key) => (
                  <div
                    key={key.id}
                    style={{
                      background: 'var(--color-surface-container)',
                      border: '1px solid var(--color-surface-container-high)',
                      borderRadius: '8px',
                      padding: '16px',
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      gap: '16px',
                      flexWrap: 'wrap',
                    }}
                  >
                    <div style={{ minWidth: 0 }}>
                      <div style={{ fontWeight: 600, color: 'var(--text-primary)' }}>{key.name}</div>
                      <div style={{ fontSize: '13px', color: 'var(--text-secondary)', marginTop: '4px' }}>
                        Created: {key.created} • Last used: {key.last_used}
                      </div>
                    </div>
                    <ActionButton
                      variant="danger"
                      style={{ padding: '6px 12px', fontSize: '13px' }}
                      onClick={() =>
                        notify(
                          'info',
                          `API key revocation is not implemented in this build (${key.name}).`,
                        )
                      }
                    >
                      Revoke
                    </ActionButton>
                  </div>
                ))}
              </div>
            )}
          </Panel>
        </>
      )}
    </PageShell>
  );
};
