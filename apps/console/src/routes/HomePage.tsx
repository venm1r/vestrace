import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';
import { TaskWorkbench } from '../shell/TaskWorkbench';
import { ApprovalChallenge } from '../components/ApprovalChallenge';
import { CompactChat } from '../components/CompactChat';

export const HomePage: React.FC = () => {
  const [taskInput, setTaskInput] = React.useState('');

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-6)' }}>
      {/* 1. Smart Task Composer (Home Landing) */}
      <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        <h2 style={{ margin: 0, fontSize: '18px', color: 'var(--brand-cyan)' }}>Start New Durable Task</h2>
        <p style={{ margin: 0, fontSize: '14px', color: 'var(--text-secondary)' }}>
          Describe a task or goal. Vestrace will construct an authoritative ExecutionPlan and enforce RLS policies.
        </p>
        <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
          <input
            type="text"
            value={taskInput}
            onChange={(e) => setTaskInput(e.target.value)}
            placeholder="e.g. Audit workspace envelope encryption and generate security verification report"
            style={{
              flex: 1,
              padding: 'var(--space-3)',
              backgroundColor: 'var(--bg-level-1)',
              border: '1px solid var(--border-color)',
              borderRadius: 'var(--radius-md)',
              color: 'var(--text-primary)',
              fontSize: '14px',
              fontFamily: 'var(--font-sans)',
            }}
          />
          <Button
            variant="primary"
            onClick={() => {
              if (taskInput.trim()) {
                alert(`Creating durable run for: ${taskInput}`);
                setTaskInput('');
              }
            }}
          >
            Create Durable Run
          </Button>
        </div>
      </Surface>

      {/* 2. Active Run Focus */}
      <TaskWorkbench
        title="Durable Task Execution #4092"
        status="Running"
        stepSummary="Executing Step 2 of 5: Validating Row-Level Security Isolation Policies"
      />

      {/* 3. High-Priority Human Request Inbox */}
      <ApprovalChallenge
        operation="system.deploy_schema"
        resource="Production Database Cluster"
        effect="Apply migration 0090_release_orchestration_and_manifests.sql to main database"
        risk="High"
        expiryMinutes={15}
        onApprove={() => alert('Approved')}
        onReject={() => alert('Denied')}
      />

      {/* 4. Compact Interactive Chat */}
      <CompactChat />
    </div>
  );
};
