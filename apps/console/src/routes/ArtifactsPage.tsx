import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface ArtifactItem {
  id: string;
  name: string;
  mediaType: string;
  byteSize: number;
  contentHash: string;
  status: 'quarantined' | 'active' | 'archived' | 'purged';
  createdAt: string;
}

export const ArtifactsPage: React.FC = () => {
  const [artifacts] = React.useState<ArtifactItem[]>([
    {
      id: 'art_8a92f1b4',
      name: 'audit-report-2026.pdf',
      mediaType: 'application/pdf',
      byteSize: 1048576,
      contentHash: 'sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
      status: 'active',
      createdAt: '2026-08-02T10:15:00Z',
    },
    {
      id: 'art_3f810c92',
      name: 'extracted_evidence.json',
      mediaType: 'application/json',
      byteSize: 4096,
      contentHash: 'sha256:2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae',
      status: 'quarantined',
      createdAt: '2026-08-02T11:20:00Z',
    },
  ]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>Artifacts Registry</h2>
          <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
            Immutable CAS artifacts, quarantine status, and SHA-256 provenance tracking.
          </p>
        </div>
        <Button variant="primary" onClick={() => alert('Opening resumable multipart artifact uploader dialog...')}>
          Upload New Artifact
        </Button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        {artifacts.map((art) => (
          <Surface key={art.id} level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
                <span style={{ fontFamily: 'monospace', fontWeight: 700, color: 'var(--brand-cyan)' }}>{art.name}</span>
                <span style={{ fontSize: '12px', color: 'var(--text-secondary)', fontFamily: 'monospace' }}>({art.mediaType})</span>
              </div>
              <span
                style={{
                  padding: '2px 8px',
                  borderRadius: 'var(--radius-round)',
                  fontSize: '11px',
                  fontWeight: 700,
                  textTransform: 'uppercase',
                  backgroundColor: art.status === 'active' ? 'var(--semantic-success)' : 'var(--semantic-warning)',
                  color: '#000',
                }}
              >
                {art.status}
              </span>
            </div>

            <div style={{ fontSize: '13px', display: 'flex', flexDirection: 'column', gap: 'var(--space-1)', color: 'var(--text-secondary)' }}>
              <div><strong>Artifact ID:</strong> <code style={{ color: 'var(--brand-white)' }}>{art.id}</code></div>
              <div><strong>Size:</strong> {(art.byteSize / 1024).toFixed(1)} KB</div>
              <div style={{ wordBreak: 'break-all' }}><strong>Hash:</strong> <code style={{ fontSize: '12px' }}>{art.contentHash}</code></div>
            </div>

            <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
              <Button variant="secondary" onClick={() => alert(`Provenance details for artifact ${art.id}:\n- Content Hash: ${art.contentHash}\n- Inspection Status: Passed\n- Source Run: run_4092`)}>
                Inspect Provenance
              </Button>
              <Button variant="ghost" onClick={() => alert(`Initiating export stream for ${art.name}...`)}>
                Download Safe Copy
              </Button>
            </div>
          </Surface>
        ))}
      </div>
    </div>
  );
};
