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

  const items = artifacts ?? [];

  return (
    <PageShell>
      <PageHeader
        title="Artifacts Repository"
        description="Artifact metadata, version lineage, provenance verification, and run relationships."
        actions={
          <ActionButton
            icon="download"
            disabled={items.length === 0}
            onClick={() => notify('info', 'Artifact export is not implemented in this build.')}
          >
            Export Selected
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

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
                      // The full digest on hover: the cell is truncated to keep
                      // the row readable, but a provenance check needs all 64
                      // characters.
                      title={artifact.content_sha256}
                    >
                      {shortChecksum(artifact.content_sha256)}
                    </td>
                    <td style={{ padding: '16px 24px', textAlign: 'right' }}>
                      <ActionButton
                        variant="quiet"
                        style={{ padding: '6px 12px', fontSize: '13px' }}
                        onClick={() =>
                          notify(
                            'info',
                            `Artifact download is not implemented in this build (${artifact.name}).`,
                          )
                        }
                      >
                        Preview &amp; Download
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
