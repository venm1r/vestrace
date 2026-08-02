import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export const CompactChat: React.FC = () => {
  const [messages, setMessages] = React.useState([
    { sender: 'User', text: 'Execute safety verification audit on migration 0090.' },
    { sender: 'System', text: 'Task #4092 created. Step 1: Pre-flight check passed. Step 2: Validating RLS isolation...' },
  ]);
  const [input, setInput] = React.useState('');

  const handleSend = () => {
    if (!input.trim()) return;
    setMessages([...messages, { sender: 'User', text: input }]);
    setInput('');
  };

  return (
    <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
      <h3 style={{ margin: 0, fontSize: '16px', color: 'var(--text-primary)' }}>Compact Chat (Run Interaction)</h3>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)', maxHeight: '180px', overflowY: 'auto' }}>
        {messages.map((msg, idx) => (
          <div key={idx} style={{ fontSize: '13px', padding: 'var(--space-2)', backgroundColor: 'var(--bg-level-1)', borderRadius: 'var(--radius-sm)' }}>
            <strong style={{ color: msg.sender === 'User' ? 'var(--brand-cyan)' : 'var(--brand-white)' }}>{msg.sender}: </strong>
            <span style={{ color: 'var(--text-primary)' }}>{msg.text}</span>
          </div>
        ))}
      </div>
      <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Send a instruction message..."
          style={{
            flex: 1,
            backgroundColor: 'var(--bg-level-0)',
            border: '1px solid var(--border-color)',
            color: 'var(--text-primary)',
            padding: 'var(--space-2) var(--space-3)',
            borderRadius: 'var(--radius-md)',
          }}
        />
        <Button variant="primary" onClick={handleSend}>Send</Button>
      </div>
    </Surface>
  );
};
