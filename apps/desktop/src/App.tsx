import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent } from 'react';
import { LogicalSize } from '@tauri-apps/api/dpi';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useSessionStore } from './store/session';
import { useUpdate } from './useUpdate';
import type { HistoryItem, RuntimeInfo, SessionSnapshot } from './types';

const WIN_WIDTH = 360;
const WIN_MIN_H = 200;
const WIN_MAX_H = 720;

type StatusKind =
  | 'idle'
  | 'recording'
  | 'transcribing'
  | 'refining'
  | 'injecting'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'loading';

type StatusCopy = {
  title: string;
};

function statusKind(status: string | undefined): StatusKind {
  if (!status) return 'loading';
  switch (status) {
    case 'idle':
    case 'recording':
    case 'transcribing':
    case 'refining':
    case 'injecting':
    case 'completed':
    case 'failed':
    case 'cancelled':
      return status;
    default:
      return 'idle';
  }
}

function statusCopy(kind: StatusKind): StatusCopy {
  switch (kind) {
    case 'loading':
      return { title: 'Starting…' };
    case 'idle':
      return { title: 'Ready' };
    case 'recording':
      return { title: 'Listening…' };
    case 'transcribing':
      return { title: 'Transcribing…' };
    case 'refining':
      return { title: 'Refining…' };
    case 'injecting':
      return { title: 'Pasting…' };
    case 'completed':
      return { title: 'Inserted' };
    case 'failed':
      return { title: 'Failed' };
    case 'cancelled':
      return { title: 'Cancelled' };
  }
}

/** Soft 6-petal star — same mark as desktop icon / overlay orb. */
function HeaderMark({ live }: { live: boolean }) {
  return (
    <svg className={`header-mark ${live ? 'is-live' : ''}`} viewBox="0 0 64 64" aria-hidden>
      <defs>
        <radialGradient id="hmStarGrad" cx="36%" cy="28%" r="72%">
          <stop offset="0%" stopColor="#ffd4b0" />
          <stop offset="35%" stopColor="#f0a06a" />
          <stop offset="75%" stopColor="#e8905c" />
          <stop offset="100%" stopColor="#c46a3a" />
        </radialGradient>
      </defs>
      <circle cx="32" cy="32" r="30" className="hm-halo hm-halo-outer" />
      <circle cx="32" cy="32" r="24" className="hm-halo hm-halo-mid" />
      <circle cx="32" cy="32" r="18" className="hm-halo hm-halo-inner" />
      <path
        className="hm-star"
        fill="url(#hmStarGrad)"
        d="M32.00,10.00 L33.14,10.19 L34.23,10.75 L35.23,11.61 L36.11,12.69 L36.85,13.88 L37.50,15.09 L38.06,16.21 L38.60,17.17 L39.17,17.92 L39.83,18.44 L40.61,18.75 L41.54,18.86 L42.65,18.85 L43.90,18.78 L45.26,18.74 L46.67,18.79 L48.05,19.01 L49.29,19.44 L50.32,20.11 L51.05,21.00 L51.46,22.09 L51.52,23.31 L51.28,24.60 L50.78,25.90 L50.12,27.15 L49.39,28.30 L48.71,29.35 L48.15,30.30 L47.78,31.17 L47.66,32.00 L47.78,32.83 L48.15,33.70 L48.71,34.65 L49.39,35.70 L50.12,36.85 L50.78,38.10 L51.28,39.40 L51.52,40.69 L51.46,41.91 L51.05,43.00 L50.32,43.89 L49.29,44.56 L48.05,44.99 L46.67,45.21 L45.26,45.26 L43.90,45.22 L42.65,45.15 L41.54,45.14 L40.61,45.25 L39.83,45.56 L39.17,46.08 L38.60,46.83 L38.06,47.79 L37.50,48.91 L36.85,50.12 L36.11,51.31 L35.23,52.39 L34.23,53.25 L33.14,53.81 L32.00,54.00 L30.86,53.81 L29.77,53.25 L28.77,52.39 L27.89,51.31 L27.15,50.12 L26.50,48.91 L25.94,47.79 L25.40,46.83 L24.83,46.08 L24.17,45.56 L23.39,45.25 L22.46,45.14 L21.35,45.15 L20.10,45.22 L18.74,45.26 L17.33,45.21 L15.95,44.99 L14.71,44.56 L13.68,43.89 L12.95,43.00 L12.54,41.91 L12.48,40.69 L12.72,39.40 L13.22,38.10 L13.88,36.85 L14.61,35.70 L15.29,34.65 L15.85,33.70 L16.22,32.83 L16.34,32.00 L16.22,31.17 L15.85,30.30 L15.29,29.35 L14.61,28.30 L13.88,27.15 L13.22,25.90 L12.72,24.60 L12.48,23.31 L12.54,22.09 L12.95,21.00 L13.68,20.11 L14.71,19.44 L15.95,19.01 L17.33,18.79 L18.74,18.74 L20.10,18.78 L21.35,18.85 L22.46,18.86 L23.39,18.75 L24.17,18.44 L24.83,17.92 L25.40,17.17 L25.94,16.21 L26.50,15.09 L27.15,13.88 L27.89,12.69 L28.77,11.61 L29.77,10.75 L30.86,10.19 Z"
      />
    </svg>
  );
}

function IconHistory({ size = 14 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden>
      <circle cx="12" cy="12" r="8" stroke="currentColor" strokeWidth="1.75" />
      <path d="M12 8v4.5l3 1.5" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" />
    </svg>
  );
}

function IconAlert({ size = 16 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden>
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.75" />
      <path d="M12 8v5" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" />
      <circle cx="12" cy="16" r="0.9" fill="currentColor" />
    </svg>
  );
}

function IconSettings({ size = 14 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden>
      <circle cx="12" cy="12" r="3.25" stroke="currentColor" strokeWidth="1.75" />
      <path
        d="M12 3.5v2.2M12 18.3v2.2M20.5 12h-2.2M5.7 12H3.5M18 6l-1.6 1.6M7.6 16.4 6 18M18 18l-1.6-1.6M7.6 7.6 6 6"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
      />
    </svg>
  );
}

function IconCopy({ size = 14 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden>
      <rect x="8" y="8" width="11" height="11" rx="2" stroke="currentColor" strokeWidth="1.75" />
      <path
        d="M6 15H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v1"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
      />
    </svg>
  );
}

export default function App() {
  const {
    runtime,
    session,
    lastText,
    apiOnline,
    error,
    setRuntime,
    setSession,
    setLastText,
    setApiOnline,
    setError,
  } = useSessionStore();
  const [copied, setCopied] = useState(false);
  const shellRef = useRef<HTMLDivElement>(null);
  const update = useUpdate();

  function applySession(snap: SessionSnapshot) {
    setSession(snap);
    const text = snap.finalText?.trim();
    if (text) {
      setLastText(text);
    }
  }

  async function refreshApiHealth() {
    try {
      const ok = await invoke<boolean>('check_api_health');
      setApiOnline(ok);
    } catch {
      setApiOnline(false);
    }
  }

  async function openHistory() {
    try {
      const win = await WebviewWindow.getByLabel('history');
      if (!win) {
        setError('History window unavailable');
        return;
      }
      await win.show();
      await win.setFocus();
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function openSettings() {
    try {
      const win = await WebviewWindow.getByLabel('settings');
      if (!win) {
        setError('Settings window unavailable');
        return;
      }
      await win.show();
      await win.setFocus();
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function minimizeWindow() {
    // Minimize to tray (not taskbar) so Show Voice / tray can restore it.
    try {
      await getCurrentWindow().hide();
    } catch {
      /* ignore */
    }
  }

  async function hideToTray() {
    try {
      await getCurrentWindow().hide();
    } catch {
      /* ignore */
    }
  }

  async function copyLastText() {
    const text = lastText?.trim();
    if (!text) return;
    try {
      await invoke('copy_text', { text });
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function startCornerResize(e: PointerEvent<HTMLDivElement>) {
    e.preventDefault();
    e.stopPropagation();
    try {
      await getCurrentWindow().startResizeDragging('SouthEast');
    } catch {
      /* ignore */
    }
  }

  useLayoutEffect(() => {
    const shell = shellRef.current;
    if (!shell) return;

    let cancelled = false;
    const fit = async () => {
      const lastBlock = shell.querySelector<HTMLElement>('.last-block');
      const lastText = shell.querySelector<HTMLElement>('.last-text');
      const prevBlockFlex = lastBlock?.style.flex ?? '';
      const prevTextOverflow = lastText?.style.overflow ?? '';
      const prevTextFlex = lastText?.style.flex ?? '';
      const prevShellMin = shell.style.minHeight;
      const prevShellH = shell.style.height;

      shell.style.minHeight = '0';
      shell.style.height = 'auto';
      if (lastBlock) lastBlock.style.flex = '0 0 auto';
      if (lastText) {
        lastText.style.flex = '0 0 auto';
        lastText.style.overflow = 'visible';
      }

      const needed = Math.ceil(shell.scrollHeight);

      shell.style.minHeight = prevShellMin;
      shell.style.height = prevShellH;
      if (lastBlock) lastBlock.style.flex = prevBlockFlex;
      if (lastText) {
        lastText.style.flex = prevTextFlex;
        lastText.style.overflow = prevTextOverflow;
      }

      const height = Math.min(WIN_MAX_H, Math.max(WIN_MIN_H, needed));
      if (cancelled) return;
      try {
        const win = getCurrentWindow();
        const factor = await win.scaleFactor();
        const inner = await win.innerSize();
        const width = Math.max(320, Math.round(inner.width / factor));
        await win.setSize(new LogicalSize(width || WIN_WIDTH, height));
      } catch {
        /* ignore */
      }
    };

    const raf = window.requestAnimationFrame(() => {
      void fit();
    });
    return () => {
      cancelled = true;
      window.cancelAnimationFrame(raf);
    };
  }, [lastText, apiOnline, error, session?.canInsert]);

  useEffect(() => {
    void (async () => {
      try {
        const info = await invoke<RuntimeInfo>('get_runtime_info');
        const snap = await invoke<SessionSnapshot>('get_session_status');
        setRuntime(info);
        applySession(snap);
        setError(null);
        await refreshApiHealth();

        if (!useSessionStore.getState().lastText) {
          const items = await invoke<HistoryItem[]>('list_history');
          const fromHistory = items[0]?.text?.trim();
          if (fromHistory) {
            setLastText(fromHistory);
          }
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    })();

    let unlisten: (() => void) | undefined;
    void listen<SessionSnapshot>('dictation://status', (event) => {
      applySession(event.payload);
      setError(null);
    }).then((fn) => {
      unlisten = fn;
    });

    const poll = window.setInterval(() => {
      void invoke<SessionSnapshot>('get_session_status')
        .then((snap) => {
          applySession(snap);
          setError(null);
        })
        .catch(() => {
          // ignore transient poll errors
        });
    }, 250);

    const healthPoll = window.setInterval(() => {
      void refreshApiHealth();
    }, 5000);

    return () => {
      unlisten?.();
      window.clearInterval(poll);
      window.clearInterval(healthPoll);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const kind = statusKind(session?.status);
  const status = statusCopy(kind);
  const recording = kind === 'recording';
  const noField = session?.canInsert === false;
  const canCopy = Boolean(lastText?.trim()) && !noField;

  return (
    <div ref={shellRef} className={`shell state-${kind}`}>
      <header className="titlebar" data-tauri-drag-region>
        <div className="brand-block" data-tauri-drag-region>
          <HeaderMark live={recording} />
          <div className="brand-copy" data-tauri-drag-region>
            <span className="brand-name">{runtime?.appName ?? 'Voice'}</span>
            <p className={`status-line ${recording ? 'is-live' : ''}`}>{status.title}</p>
          </div>
        </div>
        <div className="window-controls">
          <button type="button" className="win-btn" aria-label="Minimize" onClick={() => void minimizeWindow()}>
            ─
          </button>
          <button type="button" className="win-btn" aria-label="Hide to tray" onClick={() => void hideToTray()}>
            ×
          </button>
        </div>
      </header>

      {apiOnline === false ? (
        <div className="banner warn" role="status">
          <IconAlert size={14} />
          <span>API offline — start local API to dictate</span>
        </div>
      ) : null}

      {update.stage.kind === 'available' ? (
        <div className="banner update" role="status">
          <span>Version {update.stage.version} is available</span>
          <button type="button" className="btn-inline" onClick={() => void update.install()}>
            Update
          </button>
        </div>
      ) : null}

      {update.stage.kind === 'downloading' ? (
        <div className="banner update" role="status">
          <span>
            Downloading {update.stage.version}
            {update.stage.percent === null ? '…' : ` — ${update.stage.percent}%`}
          </span>
        </div>
      ) : null}

      {update.stage.kind === 'installed' ? (
        <div className="banner update" role="status">
          <span>Installed {update.stage.version} — restarting</span>
        </div>
      ) : null}

      {update.stage.kind === 'failed' ? (
        <div className="banner warn" role="alert">
          <IconAlert size={14} />
          <span>Update failed — {update.stage.message}</span>
        </div>
      ) : null}

      <main className="panel">
        <div className="action-row">
          <button type="button" className="btn-quiet" onClick={() => void openHistory()}>
            <IconHistory />
            History
          </button>
          <button type="button" className="btn-quiet" onClick={() => void openSettings()}>
            <IconSettings />
            Settings
          </button>
        </div>
      </main>

      <footer className="last-block">
        <div className="last-head">
          <span className="last-label">Last text</span>
          {canCopy ? (
            <button
              type="button"
              className="btn-copy"
              aria-label={copied ? 'Copied' : 'Copy last text'}
              title={copied ? 'Copied' : 'Copy'}
              onClick={() => void copyLastText()}
            >
              <IconCopy />
            </button>
          ) : null}
        </div>
        {noField ? (
          <p className="last-text warn">No active text field</p>
        ) : lastText ? (
          <p className="last-text">{lastText}</p>
        ) : (
          <p className="last-text muted">Focus a text field, then hold the hotkey</p>
        )}
      </footer>

      {error ? (
        <p className="error" role="alert">
          {error}
        </p>
      ) : null}

      <div
        className="resize-grip"
        aria-hidden
        onPointerDown={(e) => {
          void startCornerResize(e);
        }}
      />
    </div>
  );
}
