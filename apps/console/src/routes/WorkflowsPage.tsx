import React, { useEffect, useState } from 'react';
import { vestraceClient, WorkflowItem } from '../sdk/client';

export const WorkflowsPage: React.FC = () => {
  const [workflows, setWorkflows] = useState<WorkflowItem[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    vestraceClient.listWorkflows().then(setWorkflows).finally(() => setLoading(false));
  }, []);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '24px', fontWeight: 700, color: '#F3F6F9' }}>
            Workflow Management
          </h1>
          <p style={{ fontSize: '14px', color: 'var(--color-on-surface-variant)', marginTop: '4px' }}>
            Orchestrated multi-step agent plans, graph dependencies, and automated triggers.
          </p>
        </div>

        <button
          onClick={() => alert('Opening workflow canvas editor...')}
          style={{
            background: 'var(--color-primary)',
            color: '#ffffff',
            border: 'none',
            borderRadius: '8px',
            padding: '10px 18px',
            fontSize: '14px',
            fontWeight: 600,
            cursor: 'pointer',
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
          }}
        >
          <span className="material-symbols-outlined">add</span> Create Workflow
        </button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
        {loading ? (
          <div style={{ padding: '40px', color: 'var(--color-on-surface-variant)' }}>Loading workflows...</div>
        ) : (
          workflows.map((wf) => (
            <div
              key={wf.id}
              style={{
                background: 'var(--color-surface-container-low)',
                border: '1px solid var(--color-outline)',
                borderRadius: '8px',
                padding: '16px 24px',
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                <span className="material-symbols-outlined" style={{ fontSize: '28px', color: 'var(--color-tertiary)' }}>
                  account_tree
                </span>
                <div>
                  <h3 style={{ fontFamily: 'var(--font-display)', fontSize: '16px', fontWeight: 600, color: '#F3F6F9' }}>
                    {wf.name}
                  </h3>
                  <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', marginTop: '2px', display: 'flex', gap: '16px' }}>
                    <span>ID: {wf.id}</span>
                    <span>Steps: {wf.steps_count}</span>
                    <span>Last Run: {wf.last_run}</span>
                  </div>
                </div>
              </div>

              <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                <span
                  style={{
                    padding: '2px 8px',
                    borderRadius: '4px',
                    fontSize: '12px',
                    fontWeight: 600,
                    textTransform: 'uppercase',
                    background: 'rgba(0, 230, 118, 0.15)',
                    color: '#00e676',
                  }}
                >
                  {wf.status}
                </span>

                <button
                  onClick={() => alert(`Triggering manual run for workflow ${wf.name}`)}
                  style={{
                    background: 'var(--color-primary)',
                    color: '#ffffff',
                    border: 'none',
                    borderRadius: '6px',
                    padding: '6px 14px',
                    fontSize: '13px',
                    fontWeight: 600,
                    cursor: 'pointer',
                  }}
                >
                  Run Now
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
