import React, { useState, useEffect } from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';
import { agUiClient, AgUiEvent } from '../sdk/agUiClient';

export const CompactChat: React.FC = () => {
  const [messages, setMessages] = useState<Array<{ sender: string; text: string; timestamp: string }>>([
    {
      sender: 'User',
      text: 'Execute safety verification audit on migration 0090.',
      timestamp: '10:14:00',
    },
    {
      sender: 'System (AG-UI)',
      text: 'Task #4092 created via AG-UI gateway. Step 1: Pre-flight check passed.',
      timestamp: '10:14:02',
    },
  ]);
  const [input, setInput] = useState('');
  const [agUiActive] = useState(true);

  useEffect(() => {
    if (!agUiActive) return;
    const disconnect = agUiClient.connectEventStream((evt: AgUiEvent) => {
      setMessages((prev) => [
        ...prev,
        {
          sender: `AG-UI Event: ${evt.event}`,
          text: typeof evt.data === 'string' ? evt.data : JSON.stringify(evt.data),
          timestamp: new Date().toLocaleTimeString(),
        },
      ]);
    });
    return disconnect;
  }, [agUiActive]);

  const handleSend = async () => {
    if (!input.trim()) return;
    const userMsg = input;
    setInput('');
    setMessages((prev) => [
      ...prev,
      { sender: 'User', text: userMsg, timestamp: new Date().toLocaleTimeString() },
    ]);

    const res = await agUiClient.runAgent({ message: userMsg });
    setMessages((prev) => [
      ...prev,
      {
        sender: 'System (AG-UI)',
        text: res.message || 'Message acknowledged by AG-UI boundary.',
        timestamp: new Date().toLocaleTimeString(),
      },
    ]);
  };

  return (
    <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span className="material-symbols-outlined" style={{ color: 'var(--color-tertiary)' }}>
            forum
          </span>
          <h3 style={{ margin: 0, fontSize: '16px', color: '#F3F6F9', fontFamily: 'var(--font-display)' }}>
            Compact Chat (AG-UI Gateway)
          </h3>
        </div>
        <span
          style={{
            fontSize: '11px',
            padding: '2px 8px',
            borderRadius: '4px',
            backgroundColor: agUiActive ? 'rgba(0, 230, 118, 0.15)' : 'rgba(196, 198, 205, 0.15)',
            color: agUiActive ? '#00e676' : '#c4c6cd',
            fontWeight: 600,
          }}
        >
          {agUiActive ? 'AG-UI STREAM ACTIVE' : 'STREAM DISCONNECTED'}
        </span>
      </div>

      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 'var(--space-2)',
          maxHeight: '220px',
          overflowY: 'auto',
          padding: 'var(--space-2)',
          backgroundColor: 'var(--color-surface-container-lowest)',
          borderRadius: '8px',
          border: '1px solid var(--color-outline)',
        }}
      >
        {messages.map((m, idx) => (
          <div key={idx} style={{ fontSize: '13px', display: 'flex', flexDirection: 'column', gap: '2px' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', color: 'var(--color-on-surface-variant)', fontSize: '11px' }}>
              <strong style={{ color: m.sender.includes('User') ? 'var(--color-tertiary)' : '#F3F6F9' }}>
                {m.sender}
              </strong>
              <span>{m.timestamp}</span>
            </div>
            <div style={{ color: '#F3F6F9', fontFamily: m.sender.includes('AG-UI') ? 'var(--font-mono)' : 'var(--font-sans)', fontSize: '13px' }}>
              {m.text}
            </div>
          </div>
        ))}
      </div>

      <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleSend()}
          placeholder="Send instruction via AG-UI stream..."
          style={{
            flex: 1,
            padding: '8px 12px',
            backgroundColor: 'var(--color-surface-container-lowest)',
            border: '1px solid var(--color-outline)',
            borderRadius: '6px',
            color: '#F3F6F9',
            fontSize: '13px',
            fontFamily: 'var(--font-sans)',
            outline: 'none',
          }}
        />
        <Button variant="primary" onClick={handleSend}>
          Send
        </Button>
      </div>
    </Surface>
  );
};
