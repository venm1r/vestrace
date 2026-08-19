import React from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import {
  ActionButton,
  NoticeBanner,
  PageHeader,
  PageShell,
  Panel,
  ResourceState,
  Th,
  rowStyle,
  tableHeadRowStyle,
  tableStyle,
  useNotice,
} from '../shell/PageState';

function shortChecksum(checksum: string | undefined): string {
  if (!checksum) return '—';
  return checksum.length > 24 ? `${checksum.slice(0, 24)}…` : checksum;
}

/** Exact byte counts below a kibibyte: for a provenance record the precise
 *  size is the useful number, and "0.1 KB" is not one. */
function formatSize(bytes: number | undefined): string {
  if (bytes === undefined || Number.isNaN(bytes)) return '—';
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB'];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}

export const ArtifactsPage: React.FC = () => {
  const { data: artifacts, error, loading, reload } = useApiResource(vestraceClient.listArtifacts);
  const { notice, notify, dismiss } = useNotice();
  const [selectedArtifact, setSelectedArtifact] = React.useState<typeof items[0] | null>(null);

  const items = artifacts ?? [];

  const handleExport = () => {
    if (items.length === 0) return;
    const blob = new Blob([JSON.stringify(items, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `vestrace-artifacts-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
    notify('success', `Exported ${items.length} artifacts metadata records.`);
  };

  return (
    <PageShell>
      <PageHeader
        title="Artifacts Repository"
        description="Artifact metadata, version lineage, provenance verification, and run relationships."
        actions={
          <ActionButton
            icon="download"
            disabled={items.length === 0}
            onClick={handleExport}
          >
            Export Artifacts
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      {selectedArtifact && (
        <div
          role="dialog"
          aria-modal="true"
          style={{
            position: 'fixed',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            zIndex: 999,
            backgroundColor: 'rgba(0, 0, 0, 0.7)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            padding: 'var(--space-md)',
            backdropFilter: 'blur(4px)',
          }}
          onClick={(e) => {
            if (e.target === e.currentTarget) setSelectedArtifact(null);
          }}
        >
          <div
            style={{
              width: '100%',
              maxWidth: '560px',
              backgroundColor: 'var(--color-surface-container-high)',
              borderRadius: 'var(--radius-lg)',
              padding: '24px',
              border: '1px solid var(--color-outline-variant)',
              color: 'var(--brand-white)',
            }}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px' }}>
              <h2 style={{ fontSize: '18px', fontWeight: 600, margin: 0 }}>
                {selectedArtifact.name}
              </h2>
              <button
                type="button"
                onClick={() => setSelectedArtifact(null)}
                style={{
                  background: 'transparent',
                  border: 'none',
                  color: 'var(--color-on-surface-variant)',
                  cursor: 'pointer',
                }}
              >
                <span className="material-symbols-outlined">close</span>
              </button>
            </div>

            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', fontSize: '13px' }}>
              <div>
                <span style={{ color: 'var(--text-secondary)' }}>Artifact ID: </span>
                <span style={{ fontFamily: 'var(--font-mono)' }}>{selectedArtifact.id}</span>
              </div>
              <div>
                <span style={{ color: 'var(--text-secondary)' }}>Media Type: </span>
                <span>{selectedArtifact.media_type}</span>
              </div>
              <div>
                <span style={{ color: 'var(--text-secondary)' }}>Revision: </span>
                <span style={{ fontFamily: 'var(--font-mono)' }}>{selectedArtifact.revision_number}</span>
              </div>
              <div>
                <span style={{ color: 'var(--text-secondary)' }}>Size: </span>
                <span style={{ fontFamily: 'var(--font-mono)' }}>{formatSize(selectedArtifact.size_bytes)}</span>
              </div>
              <div>
                <span style={{ color: 'var(--text-secondary)' }}>SHA-256 Digest: </span>
                <div
                  style={{
                    fontFamily: 'var(--font-mono)',
                    fontSize: '12px',
                    wordBreak: 'break-all',
                    padding: '8px',
                    borderRadius: '4px',
                    backgroundColor: 'var(--color-surface-container-lowest)',
                    marginTop: '4px',
                  }}
                >
                  {selectedArtifact.content_sha256}
                </div>
              </div>
            </div>

            <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: '20px' }}>
              <ActionButton onClick={() => setSelectedArtifact(null)}>
                Close
              </ActionButton>
            </div>
          </div>
        </div>
      )}

      <Panel>
        <ResourceState
          loading={loading}
          error={error}
          isEmpty={items.length === 0}
          resourceName="artifacts"
          emptyMessage="No artifacts exist in this workspace."
          onRetry={reload}
        />
        {!loading && !error && items.length > 0 && (
          <div className="table-scroll">
            <table style={tableStyle}>
              <thead>
                <tr style={tableHeadRowStyle}>
                  <Th>Artifact Name</Th>
                  <Th style={{ padding: 'var(--space-sm) var(--space-md)' }}>Media Type</Th>
                  <Th style={{ padding: 'var(--space-sm) var(--space-md)' }}>Rev</Th>
                  <Th style={{ padding: 'var(--space-sm) var(--space-md)' }}>Size</Th>
                  <Th style={{ padding: 'var(--space-sm) var(--space-md)' }}>Checksum (SHA-256)</Th>
                  <Th style={{ textAlign: 'right' }}>Actions</Th>
                </tr>
              </thead>
              <tbody>
                {items.map((artifact) => (
                  <tr key={artifact.id} style={rowStyle}>
                    <td style={{ padding: '16px 24px' }}>
                      <div
                        style={{
                          fontWeight: 600,
                          color: 'var(--text-primary)',
                          display: 'flex',
                          alignItems: 'center',
                          gap: '8px',
                        }}
                      >
                        <span
                          className="material-symbols-outlined"
                          aria-hidden="true"
                          style={{ color: 'var(--color-tertiary)' }}
                        >
                          description
                        </span>
                        {artifact.name}
                      </div>
                    </td>
                    <td style={{ padding: 'var(--space-md)' }}>
                      <span
                        className="type-label"
                        style={{
                          padding: 'var(--space-xs) var(--space-sm)',
                          background: 'var(--color-surface-container-high)',
                          borderRadius: 'var(--radius-sm)',
                          whiteSpace: 'nowrap',
                        }}
                      >
                        {artifact.media_type}
                      </span>
                    </td>
                    <td
                      className="type-code"
                      style={{ padding: 'var(--space-md)', color: 'var(--text-secondary)' }}
                    >
                      {artifact.revision_number}
                    </td>
                    <td className="type-code" style={{ padding: 'var(--space-md)', whiteSpace: 'nowrap' }}>
                      {formatSize(artifact.size_bytes)}
                    </td>
                    <td
                      className="type-code"
                      style={{ padding: 'var(--space-md)', color: 'var(--text-secondary)' }}
                      title={artifact.content_sha256}
                    >
                      {shortChecksum(artifact.content_sha256)}
                    </td>
                    <td style={{ padding: '16px 24px', textAlign: 'right' }}>
                      <ActionButton
                        variant="quiet"
                        style={{ padding: '6px 12px', fontSize: '13px' }}
                        onClick={() => setSelectedArtifact(artifact)}
                      >
                        Details
                      </ActionButton>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>
    </PageShell>
  );
};
