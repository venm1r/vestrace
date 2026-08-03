import React, { useEffect, useState } from 'react';
import { vestraceClient, MetricsSummary } from '../sdk/client';
import { TaskWorkbench } from '../shell/TaskWorkbench';
import { ApprovalChallenge } from '../components/ApprovalChallenge';
import { CompactChat } from '../components/CompactChat';

export const HomePage: React.FC = () => {
  const [metrics, setMetrics] = useState<MetricsSummary | null>(null);
  const [taskInput, setTaskInput] = useState('');
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    vestraceClient.getMetricsSummary().then(setMetrics);
  }, []);

  const handleLaunch = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!taskInput.trim()) return;
    setLoading(true);
    try {
      await vestraceClient.createRun({ prompt: taskInput });
      alert(`Durable run launched for: ${taskInput}`);
      setTaskInput('');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      {/* System Health Strip */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))',
          gap: '16px',
        }}
      >
        <div
          style={{
            background: 'var(--color-surface-container-low)',
            border: '1px solid var(--color-outline)',
            borderRadius: '8px',
            padding: '16px',
          }}
        >
          <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', display: 'flex', alignItems: 'center', gap: '8px' }}>
            <span className="material-symbols-outlined" style={{ color: 'var(--color-tertiary)' }}>play_arrow</span>
            Live Runs
          </div>
          <div style={{ fontFamily: 'var(--font-display)', fontSize: '28px', fontWeight: 700, marginTop: '8px', color: '#F3F6F9' }}>
            {metrics?.live_runs ?? 2}
          </div>
        </div>

        <div
          style={{
            background: 'var(--color-surface-container-low)',
            border: '1px solid var(--color-outline)',
            borderRadius: '8px',
            padding: '16px',
          }}
        >
          <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', display: 'flex', alignItems: 'center', gap: '8px' }}>
            <span className="material-symbols-outlined" style={{ color: 'var(--color-success)' }}>smart_toy</span>
            Active Agents
          </div>
          <div style={{ fontFamily: 'var(--font-display)', fontSize: '28px', fontWeight: 700, marginTop: '8px', color: '#F3F6F9' }}>
            {metrics?.active_agents ?? 3}
          </div>
        </div>

        <div
          style={{
            background: 'var(--color-surface-container-low)',
            border: '1px solid var(--color-outline)',
            borderRadius: '8px',
            padding: '16px',
          }}
        >
          <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', display: 'flex', alignItems: 'center', gap: '8px' }}>
            <span className="material-symbols-outlined" style={{ color: 'var(--color-warning)' }}>speed</span>
            Avg Latency
          </div>
          <div style={{ fontFamily: 'var(--font-display)', fontSize: '28px', fontWeight: 700, marginTop: '8px', color: '#F3F6F9' }}>
            {metrics?.avg_latency ?? '142ms'}
          </div>
        </div>

        <div
          style={{
            background: 'var(--color-surface-container-low)',
            border: '1px solid var(--color-outline)',
            borderRadius: '8px',
            padding: '16px',
          }}
        >
          <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', display: 'flex', alignItems: 'center', gap: '8px' }}>
            <span className="material-symbols-outlined" style={{ color: 'var(--color-primary-bright)' }}>account_balance_wallet</span>
            Budget Spent
          </div>
          <div style={{ fontFamily: 'var(--font-display)', fontSize: '28px', fontWeight: 700, marginTop: '8px', color: '#F3F6F9' }}>
            {metrics?.budget_spent ?? '$4.12'} <span style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', fontWeight: 400 }}>/ {metrics?.budget_limit ?? '$50.00'}</span>
          </div>
        </div>
      </div>

      {/* Smart Task Composer */}
      <div
        style={{
          background: 'var(--color-surface-container)',
          border: '1px solid var(--color-primary)',
          borderRadius: '12px',
          padding: '24px',
          boxShadow: '0 8px 32px rgba(0, 0, 0, 0.4)',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '10px', marginBottom: '16px' }}>
          <span className="material-symbols-outlined" style={{ color: 'var(--color-tertiary)', fontSize: '24px' }}>auto_awesome</span>
          <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '20px', fontWeight: 600, color: '#F3F6F9' }}>Smart Task Composer</h2>
        </div>

        <form onSubmit={handleLaunch} style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <textarea
            value={taskInput}
            onChange={(e) => setTaskInput(e.target.value)}
            placeholder="Describe the high-level objective or task for Vestrace agent execution kernel..."
            rows={3}
            style={{
              width: '100%',
              background: 'var(--color-surface-container-lowest)',
              border: '1px solid var(--color-outline)',
              borderRadius: '8px',
              color: '#F3F6F9',
              padding: '14px',
              fontSize: '15px',
              fontFamily: 'var(--font-sans)',
              resize: 'vertical',
              outline: 'none',
            }}
          />

          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: '12px' }}>
            <div style={{ display: 'flex', gap: '8px' }}>
              <span style={{ padding: '6px 12px', background: 'var(--color-surface-container-high)', borderRadius: '6px', fontSize: '13px', color: 'var(--color-on-surface-variant)', display: 'flex', alignItems: 'center', gap: '6px' }}>
                <span className="material-symbols-outlined" style={{ fontSize: '16px' }}>smart_toy</span> Agent: Default-Agent
              </span>
              <span style={{ padding: '6px 12px', background: 'var(--color-surface-container-high)', borderRadius: '6px', fontSize: '13px', color: 'var(--color-on-surface-variant)', display: 'flex', alignItems: 'center', gap: '6px' }}>
                <span className="material-symbols-outlined" style={{ fontSize: '16px' }}>verified_user</span> Autonomy: Supervised
              </span>
            </div>

            <button
              type="submit"
              disabled={loading}
              style={{
                background: 'var(--color-primary)',
                color: '#ffffff',
                border: 'none',
                borderRadius: '8px',
                padding: '10px 20px',
                fontSize: '14px',
                fontWeight: 600,
                cursor: 'pointer',
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
              }}
            >
              <span className="material-symbols-outlined">rocket_launch</span>
              {loading ? 'Launching...' : 'Execute Task'}
            </button>
          </div>
        </form>
      </div>

      {/* Task Workbench */}
      <TaskWorkbench
        title="Durable Task Execution #4092"
        status="Running"
        stepSummary="Executing Step 2 of 5: Validating Row-Level Security Isolation Policies"
      />

      {/* Human Approvals & Compact Chat */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(360px, 1fr))', gap: '20px' }}>
        <ApprovalChallenge
          operation="system.deploy_schema"
          resource="Production Database Cluster"
          effect="Apply migration 0090_release_orchestration_and_manifests.sql to main database"
          risk="High"
          expiryMinutes={15}
          onApprove={() => alert('Approval granted. Task execution resumed.')}
          onReject={() => alert('Approval rejected. Task execution halted.')}
        />
        <CompactChat />
      </div>
    </div>
  );
};
