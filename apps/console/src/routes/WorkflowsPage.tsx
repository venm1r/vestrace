import React from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';

export const WorkflowsPage: React.FC = () => {
  const { data: workflows, error, loading } = useApiResource(vestraceClient.listWorkflows);

  if (error) {
    return (
      <div role="alert" style={{ padding: '24px' }}>
        Backend data is unavailable: {error}
      </div>
    );
  }

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
          onClick={() => alert('Workflow creation is not implemented in the P0 foundation')}
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
          (workflows ?? []).map((workflow) => (
            <div
              key={workflow.id}
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
                    {workflow.name}
                  </h3>
                  <div style={{ fontSize: '13px', color: 'var(--color-on-surface-variant)', marginTop: '2px', display: 'flex', gap: '16px' }}>
                    <span>ID: {workflow.id}</span>
                    <span>Steps: {workflow.steps_count}</span>
                    <span>Last Run: {workflow.last_run}</span>
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
                  {workflow.status}
                </span>

                <button
                  onClick={() => alert(`Workflow execution is not implemented for ${workflow.name}`)}
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
