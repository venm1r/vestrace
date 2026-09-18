import React, { useState } from 'react';
import { AgentItem, vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import { Modal } from '../design-system/primitives/Modal';
import { Button } from '../design-system/primitives/Button';
import {
  ActionButton,
  NoticeBanner,
  PageHeader,
  PageShell,
  Panel,
  ResourceState,
  describeError,
  useNotice,
} from '../shell/PageState';

export const AgentsPage: React.FC = () => {
  const { data: initialAgents, error, loading, reload } = useApiResource(vestraceClient.listAgents);
  const [agents, setAgents] = useState<AgentItem[] | null>(null);
  const { notice, notify, dismiss } = useNotice();

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [systemPrompt, setSystemPrompt] = useState('');
  const [submitting, setSubmitting] = useState(false);

  const items = agents ?? initialAgents ?? [];

  const handleCreateAgent = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      notify('warning', 'Agent name is required.');
      return;
    }
    setSubmitting(true);
    try {
      const newAgent = await vestraceClient.createAgent({
        name: name.trim(),
        description: description.trim(),
        system_prompt: systemPrompt.trim(),
      });
      setAgents([newAgent, ...items]);
      setIsModalOpen(false);
      setName('');
      setDescription('');
      setSystemPrompt('');
      notify('success', `Agent "${newAgent.name}" was registered.`);
    } catch (err: unknown) {
      const described = describeError(err, 'agent registration');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <PageShell>
      <PageHeader
        title="Agent Registry"
        description="Registered agent definitions: name, purpose, and the system prompt bound to each agent."
        actions={
          <ActionButton
            icon="smart_toy"
            onClick={() => setIsModalOpen(true)}
          >
            Register Agent
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      <Modal
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        title="Register Agent"
      >
        <form onSubmit={handleCreateAgent} style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <div>
            <label
              htmlFor="agent-name"
              style={{ display: 'block', fontSize: '13px', fontWeight: 600, color: 'var(--text-primary)', marginBottom: '6px' }}
            >
              Agent Name *
            </label>
            <input
              id="agent-name"
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Code Reviewer"
              style={{
                width: '100%',
                padding: '10px 12px',
                borderRadius: 'var(--radius-md)',
                backgroundColor: 'var(--color-surface-container)',
                border: '1px solid var(--color-outline-variant)',
                color: 'var(--brand-white)',
                fontSize: '14px',
                fontFamily: 'inherit',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div>
            <label
              htmlFor="agent-description"
              style={{ display: 'block', fontSize: '13px', fontWeight: 600, color: 'var(--text-primary)', marginBottom: '6px' }}
            >
              Description
            </label>
            <input
              id="agent-description"
              type="text"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="e.g. Reviews pull requests and assesses security invariants"
              style={{
                width: '100%',
                padding: '10px 12px',
                borderRadius: 'var(--radius-md)',
                backgroundColor: 'var(--color-surface-container)',
                border: '1px solid var(--color-outline-variant)',
                color: 'var(--brand-white)',
                fontSize: '14px',
                fontFamily: 'inherit',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div>
            <label
              htmlFor="agent-system-prompt"
              style={{ display: 'block', fontSize: '13px', fontWeight: 600, color: 'var(--text-primary)', marginBottom: '6px' }}
            >
              System Prompt
            </label>
            <textarea
              id="agent-system-prompt"
              rows={5}
              value={systemPrompt}
              onChange={(e) => setSystemPrompt(e.target.value)}
              placeholder="Instructions and persona for this agent..."
              style={{
                width: '100%',
                padding: '10px 12px',
                borderRadius: 'var(--radius-md)',
                backgroundColor: 'var(--color-surface-container)',
                border: '1px solid var(--color-outline-variant)',
                color: 'var(--brand-white)',
                fontSize: '13px',
                fontFamily: 'var(--font-mono)',
                resize: 'vertical',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '12px', marginTop: '8px' }}>
            <Button
              variant="secondary"
              onClick={() => setIsModalOpen(false)}
              disabled={submitting}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              type="submit"
              disabled={submitting}
              icon="add"
            >
              {submitting ? 'Registering...' : 'Register'}
            </Button>
          </div>
        </form>
      </Modal>

      {(loading || error || items.length === 0) && (
        <Panel>
          <ResourceState
            loading={loading}
            error={error}
            isEmpty={items.length === 0}
            resourceName="agents"
            emptyMessage="No agents are registered in this workspace."
            onRetry={reload}
          />
        </Panel>
      )}

      {items.length > 0 && (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))', gap: '16px' }}>
          {items.map((agent) => (
            <div
              key={agent.id}
              style={{
                background: 'var(--color-surface-container-low)',
                border: '1px solid var(--color-outline)',
                borderRadius: '12px',
                padding: '20px',
                display: 'flex',
                flexDirection: 'column',
                gap: '16px',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                <span
                  className="material-symbols-outlined"
                  aria-hidden="true"
                  style={{ fontSize: '32px', color: 'var(--color-tertiary)' }}
                >
                  smart_toy
                </span>
                <div style={{ minWidth: 0 }}>
                  <h2
                    style={{
                      fontFamily: 'var(--font-display)',
                      fontSize: '18px',
                      fontWeight: 600,
                      color: 'var(--text-primary)',
                      margin: 0,
                    }}
                  >
                    {agent.name}
                  </h2>
                  <span
                    style={{
                      fontSize: '12px',
                      color: 'var(--text-secondary)',
                      fontFamily: 'var(--font-mono)',
                      wordBreak: 'break-all',
                    }}
                  >
                    {agent.id}
                  </span>
                </div>
              </div>

              <p style={{ fontSize: '13px', color: 'var(--text-secondary)', margin: 0 }}>
                {agent.description || 'No description was recorded for this agent.'}
              </p>

              <details>
                <summary style={{ fontSize: '13px', color: 'var(--color-tertiary)', cursor: 'pointer' }}>
                  System prompt
                </summary>
                <pre
                  style={{
                    marginTop: '8px',
                    marginBottom: 0,
                    background: 'var(--color-surface-container-lowest)',
                    border: '1px solid var(--color-outline)',
                    borderRadius: '8px',
                    padding: '12px',
                    fontFamily: 'var(--font-mono)',
                    fontSize: '12px',
                    color: 'var(--color-on-surface)',
                    whiteSpace: 'pre-wrap',
                    wordBreak: 'break-word',
                    maxHeight: '220px',
                    overflowY: 'auto',
                  }}
                >
                  {agent.system_prompt || '(empty)'}
                </pre>
              </details>
            </div>
          ))}
        </div>
      )}
    </PageShell>
  );
};
