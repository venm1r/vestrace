import React, { useEffect, useRef, useState } from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';
import { agUiClient, type AgUiEvent, type AgUiStreamStatus } from '../sdk/agUiClient';

interface ChatMessage {
  id: string;
  sender: string;
  kind: 'operator' | 'gateway' | 'error';
  text: string;
  timestamp: string;
}

const STREAM_PRESENTATION: Record<AgUiStreamStatus, { label: string; color: string }> = {
  connecting: { label: 'AG-UI STREAM CONNECTING', color: 'var(--color-warning)' },
  open: { label: 'AG-UI STREAM ACTIVE', color: 'var(--color-success)' },
  closed: { label: 'AG-UI STREAM UNAVAILABLE', color: 'var(--color-error)' },
};

let messageCounter = 0;
function nextMessageId(): string {
  messageCounter += 1;
  return `msg-${messageCounter}`;
}

function timestamp(): string {
  return new Date().toLocaleTimeString();
}

export const CompactChat: React.FC = () => {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [sending, setSending] = useState(false);
  const [streamStatus, setStreamStatus] = useState<AgUiStreamStatus>('connecting');
  const logRef = useRef<HTMLDivElement | null>(null);

  const append = (message: Omit<ChatMessage, 'id' | 'timestamp'>) =>
    setMessages((previous) => [...previous, { ...message, id: nextMessageId(), timestamp: timestamp() }]);

  useEffect(() => {
    const disconnect = agUiClient.connectEventStream({
      onStatus: setStreamStatus,
      onEvent: (event: AgUiEvent) =>
        append({
          sender: `AG-UI ${event.event}`,
          kind: 'gateway',
          text: typeof event.data === 'string' ? event.data : JSON.stringify(event.data),
        }),
      onError: (message) => append({ sender: 'AG-UI gateway', kind: 'error', text: message }),
    });

    return disconnect;
    // The stream is opened once for the lifetime of the component.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const log = logRef.current;
    if (log) log.scrollTop = log.scrollHeight;
  }, [messages]);

  const handleSend = async () => {
    const message = input.trim();
    if (!message || sending) return;

    setInput('');
    setSending(true);
    append({ sender: 'Operator', kind: 'operator', text: message });

    try {
      const result = await agUiClient.runAgent({ message });
      append({
        sender: 'AG-UI gateway',
        kind: 'gateway',
        text: result.message ?? result.status ?? 'The gateway accepted the instruction.',
      });
    } catch (reason: unknown) {
      append({
        sender: 'AG-UI gateway',
        kind: 'error',
        text: reason instanceof Error ? reason.message : 'the instruction could not be delivered',
      });
    } finally {
      setSending(false);
    }
  };

  const presentation = STREAM_PRESENTATION[streamStatus];

  return (
    <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: '12px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span className="material-symbols-outlined" aria-hidden="true" style={{ color: 'var(--color-tertiary)' }}>
            forum
          </span>
          <h3
            style={{
              margin: 0,
              fontSize: '16px',
              color: 'var(--text-primary)',
              fontFamily: 'var(--font-display)',
            }}
          >
            Compact Chat (AG-UI Gateway)
          </h3>
        </div>
        <span
          role="status"
          style={{
            fontSize: '11px',
            padding: '2px 8px',
            borderRadius: '4px',
            backgroundColor: 'var(--bg-level-3)',
            color: presentation.color,
            fontWeight: 600,
            whiteSpace: 'nowrap',
          }}
        >
          {presentation.label}
        </span>
      </div>

      <div
        ref={logRef}
        role="log"
        aria-live="polite"
        aria-label="AG-UI conversation"
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 'var(--space-2)',
          minHeight: '120px',
          maxHeight: '220px',
          overflowY: 'auto',
          padding: 'var(--space-2)',
          backgroundColor: 'var(--bg-level-0)',
          borderRadius: '8px',
          border: '1px solid var(--border-color)',
        }}
      >
        {messages.length === 0 ? (
          <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>
            No AG-UI activity yet.
          </div>
        ) : (
          messages.map((message) => (
            <div key={message.id} style={{ fontSize: '13px', display: 'flex', flexDirection: 'column', gap: '2px' }}>
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  gap: '8px',
                  color: 'var(--text-secondary)',
                  fontSize: '11px',
                }}
              >
                <strong
                  style={{
                    color:
                      message.kind === 'operator'
                        ? 'var(--color-tertiary)'
                        : message.kind === 'error'
                          ? 'var(--color-error)'
                          : 'var(--text-primary)',
                  }}
                >
                  {message.sender}
                </strong>
                <span>{message.timestamp}</span>
              </div>
              <div
                style={{
                  color: message.kind === 'error' ? 'var(--color-error)' : 'var(--text-primary)',
                  fontFamily: message.kind === 'operator' ? 'var(--font-sans)' : 'var(--font-mono)',
                  fontSize: '13px',
                  wordBreak: 'break-word',
                }}
              >
                {message.text}
              </div>
            </div>
          ))
        )}
      </div>

      <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
        <label htmlFor="ag-ui-input" style={{ position: 'absolute', left: '-9999px' }}>
          Send instruction via AG-UI stream
        </label>
        <input
          id="ag-ui-input"
          type="text"
          value={input}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              void handleSend();
            }
          }}
          placeholder="Send instruction via AG-UI stream..."
          style={{
            flex: 1,
            minWidth: 0,
            padding: '8px 12px',
            backgroundColor: 'var(--bg-level-0)',
            border: '1px solid var(--border-color)',
            borderRadius: '6px',
            color: 'var(--text-primary)',
            fontSize: '13px',
          }}
        />
        <Button variant="primary" onClick={() => void handleSend()} disabled={sending || !input.trim()}>
          {sending ? 'Sending...' : 'Send'}
        </Button>
      </div>
    </Surface>
  );
};
