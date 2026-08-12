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
            onClick={() => notify('info', 'Artifact export is not implemented in the P0 foundation.')}
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
                  <Th style={{ padding: '12px 16px' }}>Kind</Th>
                  <Th style={{ padding: '12px 16px' }}>Size</Th>
                  <Th style={{ padding: '12px 16px' }}>Checksum (SHA-256)</Th>
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
                    <td style={{ padding: '16px' }}>
                      <span
                        style={{
                          padding: '4px 8px',
                          background: 'var(--color-surface-container-high)',
                          borderRadius: '4px',
                          fontSize: '12px',
                        }}
                      >
                        {artifact.kind}
                      </span>
                    </td>
                    <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>{artifact.size}</td>
                    <td
                      style={{
                        padding: '16px',
                        fontFamily: 'var(--font-mono)',
                        fontSize: '12px',
                        color: 'var(--text-secondary)',
                      }}
                      title={artifact.checksum}
                    >
                      {shortChecksum(artifact.checksum)}
                    </td>
                    <td style={{ padding: '16px 24px', textAlign: 'right' }}>
                      <ActionButton
                        variant="quiet"
                        style={{ padding: '6px 12px', fontSize: '13px' }}
                        onClick={() =>
                          notify(
                            'info',
                            `Artifact download is not implemented in the P0 foundation (${artifact.name}).`,
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
