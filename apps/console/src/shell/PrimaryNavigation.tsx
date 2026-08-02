import React from 'react';

export interface PrimaryNavigationProps {
  activeItem: string;
  onSelect: (item: string) => void;
  humanRequestCount?: number;
}

export const PrimaryNavigation: React.FC<PrimaryNavigationProps> = ({
  activeItem,
  onSelect,
  humanRequestCount = 1,
}) => {
  const sections = [
    {
      title: 'Workspace',
      items: [
        { id: 'home', label: 'Home' },
        { id: 'runs', label: 'Runs' },
        { id: 'artifacts', label: 'Artifacts' },
      ],
    },
    {
      title: 'Automation',
      items: [
        { id: 'agents', label: 'Agents' },
        { id: 'workflows', label: 'Workflows' },
        { id: 'triggers', label: 'Triggers' },
      ],
    },
    {
      title: 'System',
      items: [
        { id: 'connections', label: 'Connections' },
        { id: 'models', label: 'Models' },
        { id: 'evaluations', label: 'Evaluations' },
        { id: 'audit', label: 'Audit' },
        { id: 'settings', label: 'Settings' },
      ],
    },
  ];

  return (
    <aside
      style={{
        width: '240px',
        backgroundColor: 'var(--bg-level-1)',
        borderRight: '1px solid var(--border-color)',
        padding: 'var(--space-4)',
        display: 'flex',
        flexDirection: 'column',
        gap: 'var(--space-5)',
        height: '100vh',
        boxSizing: 'border-box',
      }}
    >
      {/* Brand Header */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
        <div
          style={{
            width: '28px',
            height: '28px',
            borderRadius: 'var(--radius-md)',
            backgroundColor: 'var(--brand-blue)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            color: '#fff',
            fontWeight: 800,
            fontSize: '16px',
          }}
        >
          V
        </div>
        <div>
          <div style={{ fontWeight: 700, fontSize: '15px', color: 'var(--text-primary)' }}>VESTRACE</div>
          <div style={{ fontSize: '10px', color: 'var(--brand-cyan)', letterSpacing: '0.5px' }}>
            AI EXECUTION KERNEL
          </div>
        </div>
      </div>

      {/* Navigation Sections */}
      <nav style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
        {sections.map((section) => (
          <div key={section.title}>
            <div
              style={{
                fontSize: '11px',
                fontWeight: 700,
                textTransform: 'uppercase',
                color: 'var(--text-secondary)',
                marginBottom: 'var(--space-2)',
                letterSpacing: '0.5px',
              }}
            >
              {section.title}
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-1)' }}>
              {section.items.map((item) => {
                const isActive = activeItem === item.id;
                return (
                  <button
                    key={item.id}
                    onClick={() => onSelect(item.id)}
                    style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      padding: 'var(--space-2) var(--space-3)',
                      borderRadius: 'var(--radius-md)',
                      backgroundColor: isActive ? 'var(--bg-level-2)' : 'transparent',
                      color: isActive ? 'var(--brand-cyan)' : 'var(--text-primary)',
                      border: 'none',
                      fontSize: '14px',
                      fontWeight: isActive ? 600 : 400,
                      textAlign: 'left',
                      cursor: 'pointer',
                    }}
                  >
                    <span>{item.label}</span>
                    {item.id === 'runs' && humanRequestCount > 0 && (
                      <span
                        style={{
                          backgroundColor: 'var(--semantic-warning)',
                          color: '#000',
                          fontSize: '11px',
                          fontWeight: 700,
                          padding: '1px 6px',
                          borderRadius: 'var(--radius-round)',
                        }}
                      >
                        {humanRequestCount}
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </nav>
    </aside>
  );
};
