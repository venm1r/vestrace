import React from 'react';
import { Link, useLocation } from 'react-router-dom';
import { PageHeader, PageShell, Panel, StatusMessage } from '../shell/PageState';

export const NotFoundPage: React.FC = () => {
  const location = useLocation();

  return (
    <PageShell>
      <PageHeader
        title="Route not found"
        description="The console has no view registered for this address."
      />
      <Panel>
        <StatusMessage
          tone="warning"
          role="alert"
          title={`No console view is mapped to ${location.pathname}`}
          detail="Use the primary navigation to reach a registered surface."
          action={
            <Link
              to="/"
              style={{
                display: 'inline-block',
                background: 'var(--color-primary)',
                color: '#ffffff',
                borderRadius: '8px',
                padding: '10px 18px',
                fontSize: '14px',
                fontWeight: 600,
                textDecoration: 'none',
              }}
            >
              Go to Home
            </Link>
          }
        />
      </Panel>
    </PageShell>
  );
};
