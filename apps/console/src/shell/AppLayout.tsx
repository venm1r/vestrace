import React from 'react';
import { PrimaryNavigation } from './PrimaryNavigation';
import { InspectorDrawer } from './InspectorDrawer';
import { HomePage } from '../routes/HomePage';
import { RunsPage } from '../routes/RunsPage';
import { ArtifactsPage } from '../routes/ArtifactsPage';
import { AgentsPage } from '../routes/AgentsPage';
import { WorkflowsPage } from '../routes/WorkflowsPage';
import { TriggersPage } from '../routes/TriggersPage';
import { ConnectionsPage } from '../routes/ConnectionsPage';
import { ModelsPage } from '../routes/ModelsPage';
import { EvaluationsPage } from '../routes/EvaluationsPage';
import { AuditPage } from '../routes/AuditPage';
import { SettingsPage } from '../routes/SettingsPage';

export const AppLayout: React.FC = () => {
  const [activeNav, setActiveNav] = React.useState('home');
  const [inspectorOpen, setInspectorOpen] = React.useState(false);

  const renderMainContent = () => {
    switch (activeNav) {
      case 'home':
        return <HomePage />;
      case 'runs':
        return <RunsPage />;
      case 'artifacts':
        return <ArtifactsPage />;
      case 'agents':
        return <AgentsPage />;
      case 'workflows':
        return <WorkflowsPage />;
      case 'triggers':
        return <TriggersPage />;
      case 'connections':
        return <ConnectionsPage />;
      case 'models':
        return <ModelsPage />;
      case 'evaluations':
        return <EvaluationsPage />;
      case 'audit':
        return <AuditPage />;
      case 'settings':
        return <SettingsPage />;
      default:
        return null;
    }
  };

  return (
    <div style={{ display: 'flex', minHeight: '100vh', backgroundColor: 'var(--bg-level-0)' }} data-theme="dark">
      {/* 13. Navigation Structure */}
      <PrimaryNavigation activeItem={activeNav} onSelect={setActiveNav} humanRequestCount={1} />

      {/* 12. Desktop Layout: Task Workspace */}
      <main style={{ flex: 1, padding: 'var(--space-6)', display: 'flex', flexDirection: 'column', gap: 'var(--space-6)', maxWidth: '960px' }}>
        {/* Top Operational Status Header */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div>
            <h1 style={{ margin: 0, fontSize: '24px', color: 'var(--text-primary)', textTransform: 'capitalize' }}>
              {activeNav} Workspace
            </h1>
            <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
              Durable Runs. Scoped Authority. Safe Effects. Verifiable Outcomes.
            </p>
          </div>
          <button
            onClick={() => setInspectorOpen(!inspectorOpen)}
            style={{
              backgroundColor: 'var(--bg-level-2)',
              color: 'var(--brand-white)',
              border: '1px solid var(--border-color)',
              padding: 'var(--space-2) var(--space-4)',
              borderRadius: 'var(--radius-md)',
              fontWeight: 600,
              cursor: 'pointer',
            }}
          >
            {inspectorOpen ? 'Close Inspector' : 'Open Inspector'}
          </button>
        </div>

        {renderMainContent()}
      </main>

      {/* 21. Contextual Inspector Drawer */}
      <InspectorDrawer
        isOpen={inspectorOpen}
        onClose={() => setInspectorOpen(false)}
        runId="run_7f81a4b9"
        runStatus="Running"
        budgetUsed="$0.042 / $10.00"
      />
    </div>
  );
};
