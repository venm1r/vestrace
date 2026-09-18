import React from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import {
  NoticeBanner,
  PageHeader,
  PageShell,
  Panel,
  ResourceState,
  useNotice,
} from '../shell/PageState';

export const ProfilePage: React.FC = () => {
  const { data: profile, error, loading, reload } = useApiResource(vestraceClient.getProfile);
  const { notice, dismiss } = useNotice();

  return (
    <PageShell>
      <PageHeader
        title="Operator Profile"
        description="Active identity, authentication posture, and workspace session scope."
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
        <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <Panel style={{ padding: '24px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '16px', marginBottom: '20px' }}>
              <div
                aria-hidden="true"
                style={{
                  width: '64px',
                  height: '64px',
                  borderRadius: '50%',
                  background: 'var(--color-primary)',
                  color: 'var(--color-on-primary)',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  fontSize: '28px',
                  fontWeight: 700,
                  fontFamily: 'var(--font-display)',
                  flexShrink: 0,
                }}
              >
                <span className="material-symbols-outlined" style={{ fontSize: '32px' }}>
                  shield_person
                </span>
              </div>
              <div style={{ minWidth: 0 }}>
                <h2
                  style={{
                    fontFamily: 'var(--font-display)',
                    fontSize: '20px',
                    fontWeight: 600,
                    color: 'var(--text-primary)',
                    margin: 0,
                  }}
                >
                  Operator Principal
                </h2>
                <p style={{ fontSize: '13px', color: 'var(--text-secondary)', margin: '4px 0 0 0', fontFamily: 'var(--font-mono)' }}>
                  {profile.principal_id}
                </p>
              </div>
            </div>

            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(260px, 1fr))', gap: '16px' }}>
              <div
                style={{
                  background: 'var(--color-surface-container)',
                  padding: '16px',
                  borderRadius: '8px',
                  border: '1px solid var(--color-surface-container-high)',
                }}
              >
                <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>Authentication Posture</div>
                <div
                  style={{
                    fontSize: '15px',
                    fontWeight: 600,
                    color: profile.authenticated ? 'var(--color-success)' : 'var(--color-warning)',
                    marginTop: '6px',
                    display: 'flex',
                    alignItems: 'center',
                    gap: '6px',
                  }}
                >
                  <span className="material-symbols-outlined" aria-hidden="true" style={{ fontSize: '18px' }}>
                    {profile.authenticated ? 'verified_user' : 'lock_open'}
                  </span>
                  {profile.authenticated ? 'Authenticated Bearer Token' : 'Unauthenticated Header'}
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
                <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>Identity Source</div>
                <div
                  style={{
                    fontSize: '14px',
                    fontWeight: 600,
                    color: 'var(--text-primary)',
                    marginTop: '6px',
                    fontFamily: 'var(--font-mono)',
                  }}
                >
                  {profile.identity_source}
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
                <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>Bound Workspace ID</div>
                <div
                  style={{
                    fontSize: '13px',
                    fontWeight: 600,
                    color: 'var(--color-tertiary)',
                    marginTop: '6px',
                    fontFamily: 'var(--font-mono)',
                    wordBreak: 'break-all',
                  }}
                >
                  {profile.workspace_id}
                </div>
              </div>
            </div>
          </Panel>
        </div>
      )}
    </PageShell>
  );
};
