import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { PrivacyMode } from '@voice/contracts';
import { useSessionStore } from './store/session';
import type { RuntimeInfo, SessionSnapshot } from './types';

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
    busy,
    error,
    setRuntime,
    setSession,
    setPrivacyMode,
    setBusy,
    setError,
  } = useSessionStore();

  useEffect(() => {
    void (async () => {
      try {
        const info = await invoke<RuntimeInfo>('get_runtime_info');
        const snap = await invoke<SessionSnapshot>('get_session_status');
        setRuntime(info);
        setSession(snap);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    })();
  }, [setError, setRuntime, setSession]);

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

  return (
    <div className="shell">
      <header className="hero">
        <p className="brand">Voice</p>
        <h1>Speak into any app</h1>
        <p className="lede">
          Windows MVP shell. Hold a hotkey, speak, get polished text — powered by DeepSeek refine.
        </p>
      </header>

      <section className="panel status-panel">
        <div className="row">
          <span className="label">Status</span>
          <span className={`badge status-${session?.status ?? 'idle'}`}>
            {session?.status ?? 'idle'}
          </span>
        </div>
        <p className="message">{session?.message ?? 'Loading…'}</p>
        {session?.sessionId ? (
          <p className="meta">Session {session.sessionId.slice(0, 8)}</p>
        ) : null}
      </section>

      <section className="panel controls">
        <button
          type="button"
          className="btn primary"
          disabled={busy || session?.status === 'recording'}
          onClick={() => void runCommand('start_dictation')}
        >
          Start (PTT stub)
        </button>
        <button
          type="button"
          className="btn"
          disabled={busy || session?.status !== 'recording'}
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
          Local keeps data on device. Cloud uses ASR + DeepSeek via API. Hybrid splits the path.
        </p>
      </section>

      <section className="panel meta-panel">
        <h2>Runtime</h2>
        <dl>
          <div>
            <dt>Platform</dt>
            <dd>{runtime?.platform ?? '—'}</dd>
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
      </section>

      {error ? <p className="error">{error}</p> : null}

      <footer className="footer">M0 foundation · audio & injection arrive in M1–M3</footer>
    </div>
  );
}
