import React from 'react';
import { Surface } from '../design-system/primitives/Surface';

export interface ApprovalChallengeProps {
  operation: string;
  resource: string;
  effect: string;
  risk: 'Low' | 'Medium' | 'High' | 'Critical';
  expiryMinutes: number;
  onApprove: () => void;
  onReject: () => void;
}

export const ApprovalChallenge: React.FC<ApprovalChallengeProps> = ({
  operation,
  resource,
  effect,
  risk,
  expiryMinutes,
  onApprove,
  onReject,
}) => {
  const getRiskBadgeColor = () => {
    switch (risk) {
      case 'Low': return 'var(--brand-cyan)';
      case 'Medium': return 'var(--semantic-warning)';
      case 'High': return 'var(--semantic-error)';
      case 'Critical': return 'var(--semantic-critical)';
    }
  };

  return (
    <Surface level={4} style={{ borderLeft: `4px solid ${getRiskBadgeColor()}`, gap: 'var(--space-3)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <h3 style={{ margin: 0, fontSize: '16px', color: 'var(--text-primary)' }}>Approval Required</h3>
        <span style={{ backgroundColor: getRiskBadgeColor(), color: '#000', padding: '2px 8px', borderRadius: 'var(--radius-round)', fontSize: '12px', fontWeight: 700 }}>
          {risk.toUpperCase()} RISK
        </span>
      </div>

      <div style={{ fontSize: '14px', display: 'flex', flexDirection: 'column', gap: 'var(--space-2)', marginTop: 'var(--space-2)' }}>
        <div><strong>Operation:</strong> <code style={{ color: 'var(--brand-cyan)' }}>{operation}</code></div>
        <div><strong>Resource:</strong> <code style={{ color: 'var(--brand-white)' }}>{resource}</code></div>
        <div><strong>Expected Effect:</strong> {effect}</div>
        <div style={{ fontSize: '12px', color: 'var(--brand-muted)' }}>Expires in {expiryMinutes} minutes</div>
      </div>

      <div style={{ display: 'flex', gap: 'var(--space-2)', marginTop: 'var(--space-3)' }}>
        <button
          onClick={onApprove}
          style={{
            backgroundColor: 'var(--semantic-success)',
            color: '#000',
            border: 'none',
            borderRadius: 'var(--radius-md)',
            padding: 'var(--space-2) var(--space-4)',
            fontWeight: 700,
            cursor: 'pointer',
          }}
        >
          Approve Operation
        </button>
        <button
          onClick={onReject}
          style={{
            backgroundColor: 'var(--bg-level-2)',
            color: 'var(--text-primary)',
            border: '1px solid var(--border-color)',
            borderRadius: 'var(--radius-md)',
            padding: 'var(--space-2) var(--space-4)',
            fontWeight: 600,
            cursor: 'pointer',
          }}
        >
          Deny Request
        </button>
      </div>
    </Surface>
  );
};
