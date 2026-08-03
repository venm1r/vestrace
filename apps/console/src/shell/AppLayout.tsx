import React from 'react';
import { NavLink, useNavigate } from 'react-router-dom';

interface AppLayoutProps {
  children: React.ReactNode;
}

export const AppLayout: React.FC<AppLayoutProps> = ({ children }) => {
  const navigate = useNavigate();

  return (
    <div style={{ display: 'flex', width: '100vw', height: '100vh', overflow: 'hidden', backgroundColor: 'var(--color-surface)' }}>
      {/* Side Navigation Bar */}
      <aside
        style={{
          width: '240px',
          backgroundColor: 'var(--color-surface-container-lowest)',
          borderRight: '1px solid var(--color-outline)',
          display: 'flex',
          flexDirection: 'column',
          height: '100vh',
          padding: '16px 12px',
          boxSizing: 'border-box',
          flexShrink: 0,
        }}
      >
        <div style={{ padding: '0 8px 20px 8px', display: 'flex', alignItems: 'center', gap: '10px' }}>
          <span className="material-symbols-outlined" style={{ color: 'var(--color-tertiary)', fontSize: '28px' }}>
            memory
          </span>
          <div>
            <div style={{ fontFamily: 'var(--font-display)', fontSize: '16px', fontWeight: 700, color: '#F3F6F9', letterSpacing: '0.05em' }}>
              VESTRACE
            </div>
            <div style={{ fontSize: '10px', color: 'var(--color-on-surface-variant)', letterSpacing: '0.5px' }}>
              EXECUTION KERNEL
            </div>
          </div>
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: '20px', flex: 1, overflowY: 'auto' }}>
          {[
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
          ].map((group) => (
            <div key={group.title} style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
              <div style={{ fontSize: '11px', textTransform: 'uppercase', color: 'var(--color-on-surface-variant)', letterSpacing: '0.1em', padding: '0 8px', fontWeight: 600 }}>
                {group.title}
              </div>
              {group.items.map((item) => (
                <NavLink
                  key={item.path}
                  to={item.path}
                  style={({ isActive }: { isActive: boolean }) => ({
                    display: 'flex',
                    alignItems: 'center',
                    gap: '10px',
                    padding: '8px 12px',
                    borderRadius: '6px',
                    fontSize: '14px',
                    fontWeight: isActive ? 600 : 400,
                    textDecoration: 'none',
                    color: isActive ? '#ffffff' : 'var(--color-on-surface-variant)',
                    backgroundColor: isActive ? 'var(--color-surface-container-high)' : 'transparent',
                  })}
                >
                  <span className="material-symbols-outlined" style={{ fontSize: '18px' }}>
                    {item.icon}
                  </span>
                  {item.label}
                </NavLink>
              ))}
            </div>
          ))}
        </div>
      </aside>

      {/* Main Content Area */}
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', height: '100vh', overflow: 'hidden' }}>
        {/* Top Header Bar */}
        <header
          style={{
            height: '60px',
            backgroundColor: 'var(--color-surface-container-low)',
            borderBottom: '1px solid var(--color-outline)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '0 24px',
            flexShrink: 0,
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
            <span style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', display: 'flex', alignItems: 'center', gap: '6px' }}>
              <span className="material-symbols-outlined" style={{ fontSize: '18px' }}>grid_view</span> Workspace: <strong style={{ color: '#F3F6F9' }}>Default-Production</strong>
            </span>
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
            <span style={{ padding: '4px 10px', background: 'rgba(0, 230, 118, 0.15)', color: 'var(--color-success)', borderRadius: '12px', fontSize: '12px', fontWeight: 600, display: 'flex', alignItems: 'center', gap: '6px' }}>
              <span style={{ width: '8px', height: '8px', borderRadius: '50%', backgroundColor: 'var(--color-success)' }}></span> Kernel Online
            </span>

            {/* Profile Avatar Button */}
            <button
              onClick={() => navigate('/profile')}
              title="View User Profile"
              style={{
                width: '36px',
                height: '36px',
                borderRadius: '50%',
                backgroundColor: 'var(--color-primary)',
                color: '#ffffff',
                border: 'none',
                cursor: 'pointer',
                fontWeight: 700,
                fontSize: '14px',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontFamily: 'var(--font-display)',
              }}
            >
              OA
            </button>
          </div>
        </header>

        {/* Viewport Scroll Container */}
        <main style={{ flex: 1, overflowY: 'auto', padding: '24px' }}>
          {children}
        </main>
      </div>
    </div>
  );
};
