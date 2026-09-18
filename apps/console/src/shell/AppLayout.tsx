import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { NavLink, useLocation, useNavigate } from 'react-router-dom';
import vestraceMark from '../assets/vestrace-mark.svg';
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
    background: 'rgba(169, 187, 205, 0.12)',
  },
  ready: {
    label: 'Kernel online',
    color: 'var(--color-success)',
    background: 'rgba(0, 230, 118, 0.1)',
  },
  degraded: {
    label: 'Kernel not ready',
    color: 'var(--color-warning)',
    background: 'rgba(255, 184, 0, 0.1)',
  },
  unreachable: {
    label: 'Kernel unreachable',
    color: 'var(--color-error)',
    background: 'rgba(255, 77, 79, 0.1)',
  },
};

const READINESS_POLL_MS = 30_000;
const MOBILE_DRAWER_QUERY = '(max-width: 1023px)';

export const AppLayout: React.FC<AppLayoutProps> = ({ children }) => {
  const location = useLocation();
  const navigate = useNavigate();
  const [readiness, setReadiness] = useState<KernelReadiness | 'checking'>('checking');
  const [navOpen, setNavOpen] = useState(false);
  const [isMobileDrawer, setIsMobileDrawer] = useState(
    () => window.matchMedia(MOBILE_DRAWER_QUERY).matches,
  );
  const sidebarRef = useRef<HTMLElement>(null);
  const drawerNavigationRef = useRef<HTMLElement>(null);
  const navToggleRef = useRef<HTMLButtonElement>(null);
  const identityProblem = describeIdentityProblem(readRequestIdentity());
  const isRunsWorkspace = /^\/runs(?:\/[^/]+)?\/?$/.test(location.pathname);

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

  useEffect(() => {
    const mediaQuery = window.matchMedia(MOBILE_DRAWER_QUERY);
    const synchronizeDrawerMode = () => {
      setIsMobileDrawer(mediaQuery.matches);
      if (!mediaQuery.matches) setNavOpen(false);
    };

    synchronizeDrawerMode();
    mediaQuery.addEventListener('change', synchronizeDrawerMode);
    return () => mediaQuery.removeEventListener('change', synchronizeDrawerMode);
  }, []);

  useEffect(() => {
    if (!isMobileDrawer || !navOpen) return;

    drawerNavigationRef.current?.querySelector<HTMLAnchorElement>('a[href]')?.focus();
  }, [isMobileDrawer, navOpen]);

  useLayoutEffect(() => {
    const sidebar = sidebarRef.current;
    if (!sidebar) return;

    if (isMobileDrawer && !navOpen) {
      sidebar.setAttribute('inert', '');
    } else {
      sidebar.removeAttribute('inert');
    }
  }, [isMobileDrawer, navOpen]);

  const closeMobileNavigation = () => {
    setNavOpen(false);
    navToggleRef.current?.focus();
  };

  useEffect(() => {
    if (!isMobileDrawer || !navOpen) return;

    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeMobileNavigation();
    };

    document.addEventListener('keydown', closeOnEscape);
    return () => document.removeEventListener('keydown', closeOnEscape);
  }, [isMobileDrawer, navOpen]);

  const readinessPresentation = READINESS_PRESENTATION[readiness];

  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content">
        Skip to main content
      </a>

      {navOpen && (
        <button
          type="button"
          className="app-scrim"
          aria-label="Close navigation"
          onClick={closeMobileNavigation}
        />
      )}

      <aside
        ref={sidebarRef}
        className="app-sidebar"
        data-open={navOpen ? 'true' : 'false'}
        aria-hidden={isMobileDrawer && !navOpen ? true : undefined}
      >
        <div className="app-brand">
          <img className="app-brand-mark" src={vestraceMark} alt="" />
          <div className="app-brand-copy">
            <div className="app-brand-wordmark">Vestrace</div>
            <div className="app-brand-tagline">AI execution kernel</div>
          </div>
        </div>

        <nav ref={drawerNavigationRef} id="primary-navigation" className="app-primary-nav" aria-label="Primary">
          {NAV_GROUPS.map((group) => (
            <div key={group.title} className="app-nav-group">
              <div className="app-nav-group-title">{group.title}</div>
              {group.items.map((item) => (
                <NavLink
                  key={item.path}
                  to={item.path}
                  end={item.path === '/'}
                  className={({ isActive }) => `app-nav-link${isActive ? ' is-active' : ''}`}
                  title={item.label}
                  onClick={() => {
                    if (isMobileDrawer) closeMobileNavigation();
                  }}
                >
                  <span className="material-symbols-outlined app-nav-icon" aria-hidden="true">
                    {item.icon}
                  </span>
                  <span>{item.label}</span>
                </NavLink>
              ))}
            </div>
          ))}
        </nav>
      </aside>

      <div className="app-shell-content">
        <header className="app-topbar">
          <div className="app-workspace-identity">
            <button
              type="button"
              ref={navToggleRef}
              className="app-nav-toggle"
              aria-label={navOpen ? 'Close navigation' : 'Open navigation'}
              aria-expanded={navOpen}
              aria-controls="primary-navigation"
              onClick={() => (navOpen ? closeMobileNavigation() : setNavOpen(true))}
            >
              <span className="material-symbols-outlined" aria-hidden="true">
                {navOpen ? 'close' : 'menu'}
              </span>
            </button>
            <span className="material-symbols-outlined app-workspace-icon" aria-hidden="true">
              grid_view
            </span>
            <span className="workspace-caption">Workspace:</span>
            <strong className="app-workspace-id">
              {readRequestIdentity().workspaceId || 'not configured'}
            </strong>
          </div>

          <div className="app-topbar-actions">
            <span
              role="status"
              className="kernel-status"
              style={{ color: readinessPresentation.color, background: readinessPresentation.background }}
            >
              <span aria-hidden="true" className="kernel-status-dot" />
              {readinessPresentation.label}
            </span>
            <button
              type="button"
              className="app-icon-button"
              onClick={() => navigate('/profile')}
              aria-label="Open operator profile"
              title="Operator profile"
            >
              <span className="material-symbols-outlined" aria-hidden="true">
                person
              </span>
            </button>
          </div>
        </header>

        <main id="main-content" className={`app-main${isRunsWorkspace ? ' app-main--runs' : ''}`}>
          {identityProblem && (
            <div role="alert" className="identity-alert">
              <span className="material-symbols-outlined" aria-hidden="true">
                warning
              </span>
              <div>
                <strong>Request identity is not configured.</strong>
                <div>{identityProblem}</div>
              </div>
            </div>
          )}
          {children}
        </main>
      </div>
    </div>
  );
};
