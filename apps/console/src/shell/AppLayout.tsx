import React from 'react';
import { PrimaryNavigation } from './PrimaryNavigation';
import { TaskWorkbench } from './TaskWorkbench';
import { SimpleListView } from './SimpleListView';
import { MemoryConsole } from '../memory/MemoryConsole';
import { ApprovalChallenge } from '../components/ApprovalChallenge';
import { CompactChat } from '../components/CompactChat';
import { InspectorDrawer } from './InspectorDrawer';
import { ArtifactsPage } from '../routes/ArtifactsPage';
import { WorkflowsPage } from '../routes/WorkflowsPage';
import { AuditPage } from '../routes/AuditPage';
import { SettingsPage } from '../routes/SettingsPage';

export const AppLayout: React.FC = () => {
  const [activeNav, setActiveNav] = React.useState('home');
  const [inspectorOpen, setInspectorOpen] = React.useState(false);

  const renderMainContent = () => {
    switch (activeNav) {
      case 'home':
      case 'runs':
        return (
          <>
            <TaskWorkbench
              title="Durable Task Execution #4092"
              status="Running"
              stepSummary="Executing Step 2 of 5: Validating Row-Level Security Isolation Policies"
            />
            <ApprovalChallenge
              operation="system.deploy_schema"
              resource="Production Database Cluster"
              effect="Apply migration 0090_release_orchestration_and_manifests.sql to main database"
              risk="High"
              expiryMinutes={15}
              onApprove={() => alert('Approved')}
              onReject={() => alert('Denied')}
            />
            <MemoryConsole
              memoryId="mem_994a02f8"
              kind="Fact"
              status="Active"
              confidence={0.96}
              content="User preference: Always enforce RLS workspace isolation on multi-tenant SQL tables."
              revisionNumber={2}
            />
            <CompactChat />
          </>
        );
      case 'artifacts':
        return <ArtifactsPage />;
      case 'agents':
        return <SimpleListView title="Agents" description="Registered agent packages, instructions, and capability boundaries." />;
      case 'workflows':
        return <WorkflowsPage />;
      case 'triggers':
        return <SimpleListView title="Triggers" description="External webhook and scheduled trigger bindings." />;
      case 'connections':
        return <SimpleListView title="Connections" description="OAuth2 connections and secret envelope credential bindings." />;
      case 'models':
        return <SimpleListView title="Models" description="AI provider models registry, cost profiles, and fallback rules." />;
      case 'evaluations':
        return <SimpleListView title="Evaluations" description="Model execution evaluations, quality metrics, and performance rollups." />;
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
