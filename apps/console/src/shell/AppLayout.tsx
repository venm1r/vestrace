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
  /** Material Symbols ligature. These match the reference screens: a different
   *  glyph for the same destination is a different product to a returning
   *  operator. */
  icon: string;
}

const NAV_GROUPS: Array<{ title: string; items: NavItem[] }> = [
  {
    title: 'Workspace',
    items: [
      { label: 'Home', path: '/', icon: 'home' },
      { label: 'Runs', path: '/runs', icon: 'terminal' },
      { label: 'Artifacts', path: '/artifacts', icon: 'database' },
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
      { label: 'Connections', path: '/connections', icon: 'cable' },
      { label: 'Models', path: '/models', icon: 'model_training' },
      { label: 'Evaluations', path: '/evaluations', icon: 'analytics' },
      { label: 'Audit', path: '/audit', icon: 'history' },
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
  // Below md the sidebar leaves the flow and becomes a drawer. It starts
  // closed, and the toggle is the only way back to navigation there.
  const [navOpen, setNavOpen] = useState(false);
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

      {navOpen && (
        <button
          type="button"
          className="app-scrim"
          aria-label="Close navigation"
          onClick={() => setNavOpen(false)}
        />
      )}

      <aside
        className="app-sidebar"
        data-open={navOpen ? 'true' : 'false'}
        style={{
          width: 'var(--layout-sidebar)',
          // `surface-container`, a tone above the page background: the spec
          // builds hierarchy from tonal layering, and the previous value
          // (`surface-container-lowest`) sat *below* the page, flattening it.
          backgroundColor: 'var(--color-surface-container)',
          borderRight: '1px solid var(--color-outline-variant)',
          display: 'flex',
          flexDirection: 'column',
          height: '100%',
          padding: 'var(--space-md) var(--space-sm)',
          flexShrink: 0,
        }}
      >
        <div
          style={{
            padding: `0 var(--space-sm) var(--space-lg) var(--space-sm)`,
            display: 'flex',
            alignItems: 'center',
            gap: 'var(--space-sm)',
          }}
        >
          <span
            aria-hidden="true"
            style={{
              width: '32px',
              height: '32px',
              borderRadius: 'var(--radius-md)',
              backgroundColor: 'var(--color-primary)',
              color: 'var(--color-on-primary)',
              display: 'inline-flex',
              alignItems: 'center',
              justifyContent: 'center',
              flexShrink: 0,
            }}
          >
            <span className="material-symbols-outlined" style={{ fontSize: '20px' }}>
              terminal
            </span>
          </span>
          <div>
            {/* Sentence case in Space Grotesk at the h3 step, coloured
                `primary` — the reference wordmark. It was previously white
                uppercase at 16px, which reads as a different product. */}
            <div
              className="type-h3"
              style={{ color: 'var(--color-primary)', fontWeight: 700, letterSpacing: '-0.01em' }}
            >
              Vestrace
            </div>
            <div className="type-label" style={{ color: 'var(--color-on-surface-variant)' }}>
              EXECUTION KERNEL
            </div>
          </div>
        </div>

        <nav
          aria-label="Primary"
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: 'var(--space-lg)',
            flex: 1,
            overflowY: 'auto',
          }}
        >
          {NAV_GROUPS.map((group) => (
            <div key={group.title} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-xs)' }}>
              <div
                className="type-label"
                style={{
                  textTransform: 'uppercase',
                  color: 'var(--brand-muted)',
                  padding: `0 var(--space-sm) var(--space-xs)`,
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
                  // Selecting a destination closes the drawer; leaving it open
                  // over the page the operator just asked for would hide it.
                  onClick={() => setNavOpen(false)}
                  style={({ isActive }) => ({
                    display: 'flex',
                    alignItems: 'center',
                    gap: 'var(--space-sm)',
                    padding: 'var(--space-sm) var(--space-md)',
                    borderRadius: 'var(--radius-md)',
                    fontSize: 'var(--text-body-size)',
                    lineHeight: 'var(--text-body-line)',
                    fontWeight: isActive ? 700 : 500,
                    textDecoration: 'none',
                    // The reference marks the active item with a right accent
                    // and a tinted container, not a solid fill.
                    color: isActive ? 'var(--color-primary)' : 'var(--color-on-surface-variant)',
                    backgroundColor: isActive ? 'var(--color-primary-container)' : 'transparent',
                    borderRight: isActive
                      ? '2px solid var(--color-primary)'
                      : '2px solid transparent',
                  })}
                >
                  <span className="material-symbols-outlined" aria-hidden="true" style={{ fontSize: '20px' }}>
                    {item.icon}
                  </span>
                  <span>{item.label}</span>
                </NavLink>
              ))}
            </div>
          ))}
        </nav>
      </aside>

      <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
        <header
          style={{
            minHeight: 'var(--layout-topbar)',
            backgroundColor: 'var(--color-surface-container-low)',
            borderBottom: '1px solid var(--color-outline-variant)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 'var(--space-md)',
            padding: `0 var(--space-md)`,
            flexShrink: 0,
          }}
        >
          <span
            className="type-body-sm"
            style={{
              color: 'var(--color-on-surface-variant)',
              display: 'flex',
              alignItems: 'center',
              gap: 'var(--space-sm)',
              minWidth: 0,
            }}
          >
            <button
              type="button"
              className="app-nav-toggle"
              aria-label={navOpen ? 'Close navigation' : 'Open navigation'}
              aria-expanded={navOpen}
              onClick={() => setNavOpen((open) => !open)}
              style={{
                background: 'transparent',
                border: '1px solid var(--color-outline-variant)',
                borderRadius: 'var(--radius-md)',
                color: 'var(--color-on-surface)',
                cursor: 'pointer',
                display: 'inline-flex',
                alignItems: 'center',
                justifyContent: 'center',
                width: '36px',
                height: '36px',
                flexShrink: 0,
              }}
            >
              <span className="material-symbols-outlined" aria-hidden="true" style={{ fontSize: '20px' }}>
                {navOpen ? 'close' : 'menu'}
              </span>
            </button>
            <span className="material-symbols-outlined" aria-hidden="true" style={{ fontSize: '18px' }}>
              grid_view
            </span>
            {/* The word is dropped on a phone; the identifier beside it is the
                part that carries information. */}
            <span className="workspace-caption" style={{ whiteSpace: 'nowrap' }}>
              Workspace:
            </span>
            <strong
              className="type-label"
              style={{
                color: 'var(--brand-white)',
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

        <main
          id="main-content"
          style={{
            flex: 1,
            overflowY: 'auto',
            // 24px is the spec's dashboard margin; a phone keeps the 16px
            // gutter so content is not squeezed into the middle third.
            padding: 'var(--space-md)',
          }}
        >
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
