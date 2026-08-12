import React, { useEffect, useState } from 'react';
import { NavLink, useNavigate } from 'react-router-dom';
import {
  describeIdentityProblem,
  getKernelReadiness,
  readRequestIdentity,
  type KernelReadiness,
} from '../sdk/client';

interface AppLayoutProps {
  children: React.ReactNode;
}

interface NavItem {
  label: string;
  path: string;
  icon: string;
}

const NAV_GROUPS: Array<{ title: string; items: NavItem[] }> = [
  {
    title: 'Workspace',
    items: [
      { label: 'Home', path: '/', icon: 'home' },
      { label: 'Runs', path: '/runs', icon: 'play_circle' },
      { label: 'Artifacts', path: '/artifacts', icon: 'folder' },
    ],
  },
  {
    title: 'Automation',
    items: [
      { label: 'Agents', path: '/agents', icon: 'smart_toy' },
      { label: 'Workflows', path: '/workflows', icon: 'account_tree' },
      { label: 'Triggers', path: '/triggers', icon: 'bolt' },
    ],
  },
  {
    title: 'System',
    items: [
      { label: 'Connections', path: '/connections', icon: 'hub' },
      { label: 'Models', path: '/models', icon: 'extension' },
      { label: 'Evaluations', path: '/evaluations', icon: 'insights' },
      { label: 'Audit', path: '/audit', icon: 'policy' },
      { label: 'Settings', path: '/settings', icon: 'settings' },
    ],
  },
];

const READINESS_PRESENTATION: Record<
  KernelReadiness | 'checking',
  { label: string; color: string; background: string }
> = {
  checking: {
    label: 'Checking kernel',
    color: 'var(--text-secondary)',
    background: 'rgba(196, 198, 205, 0.15)',
  },
  ready: {
    label: 'Kernel online',
    color: 'var(--color-success)',
    background: 'rgba(0, 230, 118, 0.15)',
  },
  degraded: {
    label: 'Kernel not ready',
    color: 'var(--color-warning)',
    background: 'rgba(255, 184, 0, 0.15)',
  },
  unreachable: {
    label: 'Kernel unreachable',
    color: 'var(--color-error)',
    background: 'rgba(255, 77, 79, 0.15)',
  },
};

const READINESS_POLL_MS = 30_000;

export const AppLayout: React.FC<AppLayoutProps> = ({ children }) => {
  const navigate = useNavigate();
  const [readiness, setReadiness] = useState<KernelReadiness | 'checking'>('checking');
  const identityProblem = describeIdentityProblem(readRequestIdentity());

  useEffect(() => {
    let active = true;

    const check = () => {
      void getKernelReadiness().then((value) => {
        if (active) setReadiness(value);
      });
    };

    check();
    const timer = window.setInterval(check, READINESS_POLL_MS);

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  const readinessPresentation = READINESS_PRESENTATION[readiness];

  return (
    <div
      style={{
        display: 'flex',
        width: '100%',
        height: '100%',
        overflow: 'hidden',
        backgroundColor: 'var(--color-surface)',
      }}
    >
      <a className="skip-link" href="#main-content">
        Skip to main content
      </a>

      <aside
        className="app-sidebar"
        style={{
          width: '240px',
          backgroundColor: 'var(--color-surface-container-lowest)',
          borderRight: '1px solid var(--color-outline)',
          display: 'flex',
          flexDirection: 'column',
          height: '100%',
          padding: '16px 12px',
          flexShrink: 0,
        }}
      >
        <div style={{ padding: '0 8px 20px 8px', display: 'flex', alignItems: 'center', gap: '10px' }}>
          <span
            className="material-symbols-outlined"
            aria-hidden="true"
            style={{ color: 'var(--color-tertiary)', fontSize: '28px' }}
          >
            memory
          </span>
          <div className="app-sidebar-label">
            <div
              style={{
                fontFamily: 'var(--font-display)',
                fontSize: '16px',
                fontWeight: 700,
                color: 'var(--text-primary)',
                letterSpacing: '0.05em',
              }}
            >
              VESTRACE
            </div>
            <div style={{ fontSize: '10px', color: 'var(--text-secondary)', letterSpacing: '0.5px' }}>
              EXECUTION KERNEL
            </div>
          </div>
        </div>

        <nav
          aria-label="Primary"
          style={{ display: 'flex', flexDirection: 'column', gap: '20px', flex: 1, overflowY: 'auto' }}
        >
          {NAV_GROUPS.map((group) => (
            <div key={group.title} style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
              <div
                className="app-sidebar-label"
                style={{
                  fontSize: '11px',
                  textTransform: 'uppercase',
                  color: 'var(--text-secondary)',
                  letterSpacing: '0.1em',
                  padding: '0 8px',
                  fontWeight: 600,
                }}
              >
                {group.title}
              </div>
              {group.items.map((item) => (
                <NavLink
                  key={item.path}
                  to={item.path}
                  end={item.path === '/'}
                  className="app-nav-link"
                  title={item.label}
                  style={({ isActive }) => ({
                    display: 'flex',
                    alignItems: 'center',
                    gap: '10px',
                    padding: '8px 12px',
                    borderRadius: '6px',
                    fontSize: '14px',
                    fontWeight: isActive ? 600 : 400,
                    textDecoration: 'none',
                    color: isActive ? 'var(--text-primary)' : 'var(--text-secondary)',
                    backgroundColor: isActive ? 'var(--color-surface-container-high)' : 'transparent',
                  })}
                >
                  <span className="material-symbols-outlined" aria-hidden="true" style={{ fontSize: '18px' }}>
                    {item.icon}
                  </span>
                  <span className="app-sidebar-label">{item.label}</span>
                </NavLink>
              ))}
            </div>
          ))}
        </nav>
      </aside>

      <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
        <header
          style={{
            minHeight: '60px',
            backgroundColor: 'var(--color-surface-container-low)',
            borderBottom: '1px solid var(--color-outline)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: '16px',
            padding: '0 24px',
            flexShrink: 0,
          }}
        >
          <span
            style={{
              fontSize: '14px',
              color: 'var(--text-secondary)',
              display: 'flex',
              alignItems: 'center',
              gap: '6px',
              minWidth: 0,
            }}
          >
            <span className="material-symbols-outlined" aria-hidden="true" style={{ fontSize: '18px' }}>
              grid_view
            </span>
            <span style={{ whiteSpace: 'nowrap' }}>Workspace:</span>
            <strong
              style={{
                color: 'var(--text-primary)',
                fontFamily: 'var(--font-mono)',
                fontSize: '13px',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
              }}
            >
              {readRequestIdentity().workspaceId || 'not configured'}
            </strong>
          </span>

          <div style={{ display: 'flex', alignItems: 'center', gap: '16px', flexShrink: 0 }}>
            <span
              role="status"
              style={{
                padding: '4px 10px',
                background: readinessPresentation.background,
                color: readinessPresentation.color,
                borderRadius: '12px',
                fontSize: '12px',
                fontWeight: 600,
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                whiteSpace: 'nowrap',
              }}
            >
              <span
                aria-hidden="true"
                style={{
                  width: '8px',
                  height: '8px',
                  borderRadius: '50%',
                  backgroundColor: readinessPresentation.color,
                }}
              />
              {readinessPresentation.label}
            </span>

            <button
              type="button"
              onClick={() => navigate('/profile')}
              aria-label="Open operator profile"
              title="Operator profile"
              style={{
                width: '36px',
                height: '36px',
                borderRadius: '50%',
                backgroundColor: 'var(--color-surface-container-high)',
                color: 'var(--text-primary)',
                border: '1px solid var(--color-outline)',
                cursor: 'pointer',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <span className="material-symbols-outlined" aria-hidden="true" style={{ fontSize: '20px' }}>
                person
              </span>
            </button>
          </div>
        </header>

        <main id="main-content" style={{ flex: 1, overflowY: 'auto', padding: '24px' }}>
          {identityProblem && (
            <div
              role="alert"
              style={{
                display: 'flex',
                alignItems: 'flex-start',
                gap: '12px',
                padding: '14px 16px',
                marginBottom: '24px',
                borderRadius: '8px',
                border: '1px solid var(--color-warning)',
                background: 'rgba(255, 184, 0, 0.08)',
                fontSize: '14px',
              }}
            >
              <span
                className="material-symbols-outlined"
                aria-hidden="true"
                style={{ color: 'var(--color-warning)' }}
              >
                warning
              </span>
              <div>
                <strong style={{ color: 'var(--text-primary)' }}>Request identity is not configured.</strong>
                <div style={{ color: 'var(--text-secondary)', marginTop: '4px' }}>{identityProblem}</div>
              </div>
            </div>
          )}
          {children}
        </main>
      </div>
    </div>
  );
};
