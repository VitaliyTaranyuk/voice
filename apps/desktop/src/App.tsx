import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { PrivacyMode } from '@voice/contracts';
import { useSessionStore } from './store/session';
import type { HistoryItem, RuntimeInfo, SessionSnapshot } from './types';

const PRIVACY_LABELS: Record<PrivacyMode, string> = {
  local: 'Local',
  hybrid: 'Hybrid',
  cloud: 'Cloud',
};

export default function App() {
  const {
    runtime,
    session,
    privacyMode,
    history,
    apiOnline,
    busy,
    error,
    setRuntime,
    setSession,
    setPrivacyMode,
    setHistory,
    setApiOnline,
    setBusy,
    setError,
  } = useSessionStore();

  async function refreshHistory() {
    try {
      const items = await invoke<HistoryItem[]>('list_history');
      setHistory(items);
    } catch {
      // history optional during early boot
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

  useEffect(() => {
    void (async () => {
      try {
        const info = await invoke<RuntimeInfo>('get_runtime_info');
        const snap = await invoke<SessionSnapshot>('get_session_status');
        setRuntime(info);
        setSession(snap);
        await refreshHistory();
        await refreshApiHealth();
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    })();

    let unlisten: (() => void) | undefined;
    void listen<SessionSnapshot>('dictation://status', (event) => {
      setSession(event.payload);
      if (event.payload.status === 'completed') {
        void refreshHistory();
      }
    }).then((fn) => {
      unlisten = fn;
    });

    const poll = window.setInterval(() => {
      void invoke<SessionSnapshot>('get_session_status')
        .then(setSession)
        .catch(() => undefined);
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

  async function runCommand(command: 'start_dictation' | 'stop_dictation' | 'cancel_dictation') {
    setBusy(true);
    setError(null);
    try {
      const snap = await invoke<SessionSnapshot>(command);
      setSession(snap);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  const recording = session?.status === 'recording';
  const peak = session?.audio?.peakAmplitude ?? 0;

  return (
    <div className="shell">
      <header className="hero">
        <p className="brand">Voice</p>
        <h1>Speak into any app</h1>
        <p className="lede">
          Focus a text field, hold <kbd>{runtime?.hotkey ?? 'Ctrl+Shift+Space'}</kbd>, speak,
          release — text is transcribed, refined (DeepSeek), and pasted.
        </p>
      </header>

      <section className={`panel status-panel ${recording ? 'recording' : ''}`}>
        <div className="row">
          <span className="label">Status</span>
          <span className={`badge status-${session?.status ?? 'idle'}`}>
            {session?.status ?? 'idle'}
          </span>
        </div>
        <p className="message">{session?.message ?? 'Loading…'}</p>
        {recording ? (
          <div className="meter" aria-hidden>
            <div className="meter-fill" style={{ width: `${Math.min(peak * 100, 100)}%` }} />
          </div>
        ) : null}
        {session?.finalText ? <p className="result">{session.finalText}</p> : null}
        {session?.appContext?.processName ? (
          <p className="meta">
            Target {session.appContext.processName}
            {session.appContext.appCategory ? ` · ${session.appContext.appCategory}` : ''}
          </p>
        ) : null}
      </section>

      <section className="panel controls">
        <button
          type="button"
          className="btn primary"
          disabled={busy || recording}
          onClick={() => void runCommand('start_dictation')}
        >
          Start
        </button>
        <button
          type="button"
          className="btn"
          disabled={busy || !recording}
          onClick={() => void runCommand('stop_dictation')}
        >
          Stop
        </button>
        <button
          type="button"
          className="btn ghost"
          disabled={busy}
          onClick={() => void runCommand('cancel_dictation')}
        >
          Cancel
        </button>
      </section>

      <section className="panel">
        <h2>Privacy</h2>
        <div className="seg">
          {(Object.keys(PRIVACY_LABELS) as PrivacyMode[]).map((mode) => (
            <button
              key={mode}
              type="button"
              className={privacyMode === mode ? 'seg-item active' : 'seg-item'}
              onClick={() => setPrivacyMode(mode)}
            >
              {PRIVACY_LABELS[mode]}
            </button>
          ))}
        </div>
        <p className="hint">
          Cloud mode uses your local API at {runtime?.apiBaseUrl ?? 'http://127.0.0.1:8787'} for ASR
          + DeepSeek.
        </p>
      </section>

      <section className="panel history-panel">
        <div className="row">
          <h2>History</h2>
          <button type="button" className="btn ghost tiny" onClick={() => void refreshHistory()}>
            Refresh
          </button>
        </div>
        {history.length === 0 ? (
          <p className="hint">No dictations yet.</p>
        ) : (
          <ul className="history-list">
            {history.slice(0, 8).map((item) => (
              <li key={item.id}>
                <p>{item.text}</p>
                <span>
                  {item.appId ?? 'app'} · {new Date(item.createdAt).toLocaleString()}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel meta-panel">
        <h2>Runtime</h2>
        <dl>
          <div>
            <dt>API</dt>
            <dd className={apiOnline ? 'ok' : 'bad'}>
              {apiOnline ? 'online' : 'offline'} · {runtime?.apiBaseUrl ?? '—'}
            </dd>
          </div>
          <div>
            <dt>LLM</dt>
            <dd>{runtime?.llmProvider ?? 'deepseek'}</dd>
          </div>
          <div>
            <dt>Version</dt>
            <dd>{runtime?.version ?? '—'}</dd>
          </div>
        </dl>
        {!apiOnline ? (
          <p className="hint warn">
            API offline. Run <code>pwsh scripts/dev-api.ps1</code> and set keys in{' '}
            <code>services/api/.env</code>.
          </p>
        ) : null}
      </section>

      {error ? <p className="error">{error}</p> : null}

      <footer className="footer">
        Start API with keys in services/api/.env · then dictate into any text field
      </footer>
    </div>
  );
}
