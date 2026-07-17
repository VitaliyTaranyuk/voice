import { StrictMode, useCallback, useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { HistoryItem, SessionSnapshot } from './types';
import './history.css';

function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return '';
  const sec = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (sec < 60) return 'just now';
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h`;
  return `${Math.floor(hr / 24)}d`;
}

function friendlyApp(appId: string | null | undefined): string {
  const raw = (appId || '').trim();
  if (!raw) return 'App';
  const base = raw.replace(/\.exe$/i, '');
  return base.charAt(0).toUpperCase() + base.slice(1);
}

function HeaderMark() {
  return (
    <svg className="header-mark" viewBox="0 0 44 44" fill="none" aria-hidden>
      <circle cx="22" cy="22" r="18" className="hm-ring" />
      <circle cx="22" cy="22" r="8" className="hm-hand" strokeWidth="1.75" />
      <path
        d="M22 16v6.5l3.5 2"
        className="hm-hand"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function IconRefresh({ size = 14 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden>
      <path
        d="M4.5 12a7.5 7.5 0 0 1 12.9-5.2L20 9.5"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M19.5 12a7.5 7.5 0 0 1-12.9 5.2L4 14.5"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path d="M20 4.5v5h-5" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M4 19.5v-5h5" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function HistoryApp() {
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<HistoryItem[]>('list_history');
      setItems(list.filter((item) => item.text.trim().length > 0));
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    void refresh();

    let unlisten: (() => void) | undefined;
    void listen<SessionSnapshot>('dictation://status', (event) => {
      if (event.payload.status === 'completed') {
        void refresh();
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [refresh]);

  async function copyItem(item: HistoryItem) {
    try {
      await invoke('copy_text', { text: item.text });
      setCopiedId(item.id);
      window.setTimeout(() => {
        setCopiedId((cur) => (cur === item.id ? null : cur));
      }, 1200);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  function toggleExpand(id: string) {
    setExpandedId((cur) => (cur === id ? null : id));
  }

  async function minimizeWindow() {
    try {
      await getCurrentWindow().hide();
    } catch {
      /* ignore */
    }
  }

  async function hideWindow() {
    try {
      await getCurrentWindow().hide();
    } catch {
      /* ignore */
    }
  }

  const statusLabel =
    items.length === 0
      ? 'No dictations yet'
      : `${items.length} ${items.length === 1 ? 'entry' : 'entries'}`;

  return (
    <div className="history-shell">
      <header className="titlebar" data-tauri-drag-region>
        <div className="brand-block" data-tauri-drag-region>
          <HeaderMark />
          <div className="brand-copy" data-tauri-drag-region>
            <span className="brand-name">History</span>
            <p className="status-line">{statusLabel}</p>
          </div>
        </div>
        <div className="window-controls">
          <button type="button" className="win-btn" aria-label="Minimize" onClick={() => void minimizeWindow()}>
            ─
          </button>
          <button type="button" className="win-btn" aria-label="Close" onClick={() => void hideWindow()}>
            ×
          </button>
        </div>
      </header>

      <div className="history-toolbar">
        <button type="button" className="btn ghost" onClick={() => void refresh()}>
          <IconRefresh />
          Refresh
        </button>
      </div>

      {error ? (
        <p className="history-error" role="alert">
          {error}
        </p>
      ) : null}

      {items.length === 0 ? (
        <p className="history-empty">Focus a text field, dictate, then revisit here</p>
      ) : (
        <ul className="history-list">
          {items.map((item) => {
            const expanded = expandedId === item.id;
            const copied = copiedId === item.id;
            return (
              <li key={item.id} className={expanded ? 'is-expanded' : undefined}>
                <button
                  type="button"
                  className="history-row"
                  onClick={() => toggleExpand(item.id)}
                  aria-expanded={expanded}
                >
                  <span className={`history-text ${expanded ? 'is-full' : ''}`}>
                    {copied ? 'Copied' : item.text}
                  </span>
                  <span className="history-meta">
                    <span>{friendlyApp(item.appId)}</span>
                    <span className="history-time">{relativeTime(item.createdAt)}</span>
                  </span>
                </button>
                {expanded ? (
                  <div className="history-actions">
                    <button
                      type="button"
                      className="btn ghost compact"
                      onClick={() => {
                        void copyItem(item);
                      }}
                    >
                      {copied ? 'Copied' : 'Copy'}
                    </button>
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <HistoryApp />
  </StrictMode>,
);
