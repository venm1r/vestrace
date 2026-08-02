import React from 'react';
import ReactDOM from 'react-dom/client';
import { TaskWorkbench } from './shell/TaskWorkbench';
import { MemoryConsole } from './memory/MemoryConsole';
import { ApprovalChallenge } from './components/ApprovalChallenge';
import { InspectorDrawer } from './shell/InspectorDrawer';

const App = () => {
  const [inspectorOpen, setInspectorOpen] = React.useState(false);

  return (
    <div style={{ padding: '24px', display: 'flex', flexDirection: 'column', gap: '24px', maxWidth: '1200px', margin: '0 auto' }}>
      <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', borderBottom: '1px solid var(--border-color)', paddingBottom: '16px' }}>
        <h1 style={{ margin: 0, fontSize: '24px', color: 'var(--brand-cyan)' }}>Vestrace Console</h1>
        <button
          onClick={() => setInspectorOpen(true)}
          style={{ padding: '8px 16px', backgroundColor: 'var(--brand-blue)', color: '#fff', border: 'none', borderRadius: '4px', cursor: 'pointer', fontWeight: 600 }}
        >
          Open Inspector
        </button>
      </header>

      <main style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
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
      </main>

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

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
