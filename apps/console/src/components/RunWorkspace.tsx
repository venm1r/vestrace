import React, { useId, useState } from 'react';
import type { RunItem } from '../sdk/client';
import { getRunStatusPresentation } from '../routes/runWorkspaceModel';
import { CompactChat } from './CompactChat';

type WorkspaceTab = 'workspace' | 'agent' | 'record';

function formatTimestamp(value: string): string {
  const timestamp = new Date(value);
  return Number.isNaN(timestamp.getTime()) ? value : timestamp.toLocaleString();
}

export const RunWorkspace: React.FC<{ run: RunItem }> = ({ run }) => {
  const [tab, setTab] = useState<WorkspaceTab>('workspace');
  const tabs: WorkspaceTab[] = ['workspace', 'agent', 'record'];
  const idPrefix = useId().replace(/:/g, '');
  const status = getRunStatusPresentation(run.status);
  const panelId = (name: WorkspaceTab) => `${idPrefix}-${name}-panel`;
  const tabId = (name: WorkspaceTab) => `${idPrefix}-${name}-tab`;
  const selectTab = (name: WorkspaceTab) => {
    setTab(name);
    document.getElementById(tabId(name))?.focus();
  };

  const handleTabKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>, index: number) => {
    let nextIndex: number | null = null;
    if (event.key === 'ArrowRight') nextIndex = (index + 1) % tabs.length;
    if (event.key === 'ArrowLeft') nextIndex = (index - 1 + tabs.length) % tabs.length;
    if (event.key === 'Home') nextIndex = 0;
    if (event.key === 'End') nextIndex = tabs.length - 1;
    if (nextIndex === null) return;

    event.preventDefault();
    selectTab(tabs[nextIndex]);
  };

  return (
    <div className="workspace-panel">
      <header style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: '12px', padding: '16px', borderBottom: '1px solid var(--color-outline-variant)' }}>
        <div style={{ minWidth: 0 }}>
          <div className="type-label" style={{ color: 'var(--text-secondary)' }}>Selected run</div>
          <h1 className="type-h1" style={{ margin: '4px 0 0', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {run.title}
          </h1>
        </div>
        <span className={`status-chip status-chip--${status.tone}`}>{status.label}</span>
      </header>

      <div className="workspace-tabs" role="tablist" aria-label="Selected run views">
        {tabs.map((name, index) => (
          <button
            key={name}
            id={tabId(name)}
            type="button"
            role="tab"
            className={`workspace-tab${tab === name ? ' is-active' : ''}`}
            aria-selected={tab === name}
            aria-controls={panelId(name)}
            tabIndex={tab === name ? 0 : -1}
            onClick={() => setTab(name)}
            onKeyDown={(event) => handleTabKeyDown(event, index)}
          >
            {name === 'workspace' ? 'Workspace' : name === 'agent' ? 'Agent' : 'Record'}
          </button>
        ))}
      </div>

      <section id={panelId('workspace')} role="tabpanel" aria-labelledby={tabId('workspace')} hidden={tab !== 'workspace'} style={{ padding: '16px' }}>
        <dl style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(170px, 1fr))', gap: '14px', margin: 0 }}>
          <div>
            <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>Run ID</dt>
            <dd className="type-code" style={{ margin: '3px 0 0', overflowWrap: 'anywhere' }}>{run.id}</dd>
          </div>
          <div>
            <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>Status</dt>
            <dd style={{ margin: '3px 0 0' }}><span className={`status-chip status-chip--${status.tone}`}>{status.label}</span></dd>
          </div>
          <div>
            <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>Version</dt>
            <dd style={{ margin: '3px 0 0' }}>{run.version}</dd>
          </div>
          <div>
            <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>Created</dt>
            <dd style={{ margin: '3px 0 0' }}><time dateTime={run.created_at}>{formatTimestamp(run.created_at)}</time></dd>
          </div>
          <div>
            <dt className="type-label" style={{ color: 'var(--text-secondary)' }}>Updated</dt>
            <dd style={{ margin: '3px 0 0' }}><time dateTime={run.updated_at}>{formatTimestamp(run.updated_at)}</time></dd>
          </div>
        </dl>
      </section>

      <section id={panelId('agent')} role="tabpanel" aria-labelledby={tabId('agent')} hidden={tab !== 'agent'} style={{ padding: '16px' }}>
        <CompactChat />
      </section>

      <section id={panelId('record')} role="tabpanel" aria-labelledby={tabId('record')} hidden={tab !== 'record'} style={{ padding: '16px' }}>
        <pre className="type-code" style={{ margin: 0, overflowX: 'auto', padding: '12px', background: 'var(--color-surface-container-lowest)', border: '1px solid var(--color-outline-variant)', borderRadius: 'var(--radius-sm)' }}>
          {JSON.stringify(run, null, 2)}
        </pre>
      </section>
    </div>
  );
};
