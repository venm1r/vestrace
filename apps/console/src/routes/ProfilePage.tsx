import React from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';

export const ProfilePage: React.FC = () => {
  const { data: profile, error, loading } = useApiResource(vestraceClient.getProfile);

  if (error) {
    return (
      <div role="alert" style={{ padding: '24px' }}>
        Backend data is unavailable: {error}
      </div>
    );
  }

  if (loading || !profile) {
    return <div style={{ padding: '40px', color: 'var(--color-on-surface-variant)' }}>Loading profile...</div>;
  }

  const initials = profile.name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase())
    .join('');

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      <div
        style={{
          background: 'var(--color-surface-container-low)',
          border: '1px solid var(--color-outline)',
          borderRadius: '12px',
          padding: '24px',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '16px', marginBottom: '20px' }}>
          <div
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
            }}
          >
            {initials}
          </div>
          <div>
            <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '22px', fontWeight: 600, color: '#F3F6F9' }}>
              {profile.name}
            </h2>
            <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)' }}>
              {profile.email} • <span style={{ color: 'var(--color-tertiary)' }}>{profile.role}</span>
            </p>
          </div>
        </div>

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))', gap: '16px' }}>
          <div style={{ background: 'var(--color-surface-container)', padding: '16px', borderRadius: '8px', border: '1px solid var(--color-surface-container-high)' }}>
            <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)' }}>MFA Security Status</div>
            <div style={{ fontSize: '16px', fontWeight: 600, color: profile.mfa_enabled ? '#00e676' : '#ff4d4f', marginTop: '6px', display: 'flex', alignItems: 'center', gap: '6px' }}>
              <span className="material-symbols-outlined">{profile.mfa_enabled ? 'verified' : 'warning'}</span>
              {profile.mfa_enabled ? 'Enforced & Active' : 'Disabled'}
            </div>
          </div>

          <div style={{ background: 'var(--color-surface-container)', padding: '16px', borderRadius: '8px', border: '1px solid var(--color-surface-container-high)' }}>
            <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)' }}>Active Operator Sessions</div>
            <div style={{ fontSize: '16px', fontWeight: 600, color: '#F3F6F9', marginTop: '6px', display: 'flex', alignItems: 'center', gap: '6px' }}>
              <span className="material-symbols-outlined">devices</span>
              {profile.active_sessions} Concurrent Sessions
            </div>
          </div>
        </div>
      </div>

      <div
        style={{
          background: 'var(--color-surface-container-low)',
          border: '1px solid var(--color-outline)',
          borderRadius: '12px',
          padding: '24px',
        }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px' }}>
          <h3 style={{ fontFamily: 'var(--font-display)', fontSize: '18px', fontWeight: 600 }}>Active API Keys & Tokens</h3>
          <button
            onClick={() => alert('API key creation is not implemented in the P0 foundation')}
            style={{
              background: 'var(--color-primary)',
              color: '#ffffff',
              border: 'none',
              borderRadius: '6px',
              padding: '8px 16px',
              fontSize: '13px',
              fontWeight: 600,
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              gap: '6px',
            }}
          >
            <span className="material-symbols-outlined">add</span> Create API Key
          </button>
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          {profile.api_keys.map((key) => (
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
              }}
            >
              <div>
                <div style={{ fontWeight: 600, color: '#F3F6F9' }}>{key.name}</div>
                <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
                  Created: {key.created} • Last Used: {key.last_used}
                </div>
              </div>
              <button
                onClick={() => alert(`API key revocation is not implemented for ${key.name}`)}
                style={{
                  background: 'transparent',
                  border: '1px solid var(--color-error)',
                  color: 'var(--color-error)',
                  borderRadius: '6px',
                  padding: '6px 12px',
                  fontSize: '13px',
                  cursor: 'pointer',
                }}
              >
                Revoke
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
