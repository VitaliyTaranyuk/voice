import { StrictMode, useCallback, useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { ApiKeySaveResult, ApiKeyStatus } from './types';
import './settings.css';

const PROVIDER_LABELS: Record<string, string> = {
  deepseek: 'DeepSeek',
  groq: 'Groq',
  openai: 'OpenAI',
  deepgram: 'Deepgram',
};

/** Cloud ASR is opt-in; transcription runs locally unless one of these is set. */
const OPTIONAL_PROVIDERS = ['groq', 'openai', 'deepgram'];

function HeaderMark() {
  return (
    <svg className="header-mark" viewBox="0 0 44 44" fill="none" aria-hidden>
      <circle cx="22" cy="22" r="18" className="hm-ring" />
      <path
        d="M22 14a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0v-5a3 3 0 0 1 3-3Z"
        className="hm-hand"
        strokeWidth="1.75"
        strokeLinejoin="round"
      />
      <path
        d="M16 21.5a6 6 0 0 0 12 0M22 27.5V30"
        className="hm-hand"
        strokeWidth="1.75"
        strokeLinecap="round"
      />
    </svg>
  );
}

function KeyRow({
  provider,
  configured,
  draft,
  busy,
  onDraft,
  onSave,
  onRemove,
}: {
  provider: string;
  configured: boolean;
  draft: string;
  busy: boolean;
  onDraft: (value: string) => void;
  onSave: () => void;
  onRemove: () => void;
}) {
  const label = PROVIDER_LABELS[provider] ?? provider;
  return (
    <div className="key-row">
      <div className="key-head">
        <span className="key-label">{label}</span>
        <span className={`key-chip ${configured ? 'is-set' : ''}`}>
          {configured ? 'Saved' : 'Not set'}
        </span>
      </div>
      <div className="key-controls">
        <input
          type="password"
          className="key-input"
          value={draft}
          spellCheck={false}
          autoComplete="off"
          placeholder={configured ? 'Stored — type to replace' : `${label} API key`}
          aria-label={`${label} API key`}
          onChange={(event) => onDraft(event.target.value)}
        />
        <button type="button" className="btn ghost compact" disabled={busy || !draft.trim()} onClick={onSave}>
          Save
        </button>
        {configured ? (
          <button type="button" className="btn ghost compact danger" disabled={busy} onClick={onRemove}>
            Remove
          </button>
        ) : null}
      </div>
    </div>
  );
}

function SettingsApp() {
  const [keys, setKeys] = useState<ApiKeyStatus[]>([]);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showOptional, setShowOptional] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setKeys(await invoke<ApiKeyStatus[]>('api_key_status'));
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  function configured(provider: string): boolean {
    return keys.find((item) => item.provider === provider)?.configured ?? false;
  }

  /** Drop the typed value as soon as it is stored — no reason to keep it in the DOM. */
  function forgetDraft(provider: string) {
    setDrafts((cur) => ({ ...cur, [provider]: '' }));
  }

  async function apply(provider: string, command: 'set_api_key' | 'clear_api_key') {
    setBusy(provider);
    setNotice(null);
    try {
      const args =
        command === 'set_api_key' ? { provider, key: drafts[provider] ?? '' } : { provider };
      const result = await invoke<ApiKeySaveResult>(command, args);
      setKeys(result.keys);
      setNotice(result.notice);
      forgetDraft(provider);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }

  async function hideWindow() {
    try {
      await getCurrentWindow().hide();
    } catch {
      /* ignore */
    }
  }

  const configuredCount = keys.filter((item) => item.configured).length;

  return (
    <div className="settings-shell">
      <header className="titlebar" data-tauri-drag-region>
        <div className="brand-block" data-tauri-drag-region>
          <HeaderMark />
          <div className="brand-copy" data-tauri-drag-region>
            <span className="brand-name">Settings</span>
            <p className="status-line">
              {configuredCount === 0 ? 'No keys stored' : `${configuredCount} stored`}
            </p>
          </div>
        </div>
        <div className="window-controls">
          <button type="button" className="win-btn" aria-label="Close" onClick={() => void hideWindow()}>
            ×
          </button>
        </div>
      </header>

      <p className="settings-note">
        Keys are kept in the Windows Credential Manager on this machine. They are never bundled
        into the installer and never leave this device except in calls to the provider.
      </p>

      {error ? (
        <p className="settings-error" role="alert">
          {error}
        </p>
      ) : null}

      {notice ? (
        <p className="settings-notice" role="status">
          {notice}
        </p>
      ) : null}

      <section className="settings-section">
        <h2 className="section-title">Text refinement</h2>
        <p className="section-hint">
          Without a DeepSeek key dictation still works — transcripts are inserted as-is, without
          refinement.
        </p>
        <KeyRow
          provider="deepseek"
          configured={configured('deepseek')}
          draft={drafts['deepseek'] ?? ''}
          busy={busy === 'deepseek'}
          onDraft={(value) => setDrafts((cur) => ({ ...cur, deepseek: value }))}
          onSave={() => void apply('deepseek', 'set_api_key')}
          onRemove={() => void apply('deepseek', 'clear_api_key')}
        />
      </section>

      <section className="settings-section">
        <button
          type="button"
          className="section-toggle"
          aria-expanded={showOptional}
          onClick={() => setShowOptional((cur) => !cur)}
        >
          <span className="section-title">Cloud speech recognition</span>
          <span className="section-caret">{showOptional ? '−' : '+'}</span>
        </button>
        {showOptional ? (
          <>
            <p className="section-hint">
              Optional. Speech recognition runs locally by default and needs no key.
            </p>
            {OPTIONAL_PROVIDERS.map((provider) => (
              <KeyRow
                key={provider}
                provider={provider}
                configured={configured(provider)}
                draft={drafts[provider] ?? ''}
                busy={busy === provider}
                onDraft={(value) => setDrafts((cur) => ({ ...cur, [provider]: value }))}
                onSave={() => void apply(provider, 'set_api_key')}
                onRemove={() => void apply(provider, 'clear_api_key')}
              />
            ))}
          </>
        ) : null}
      </section>
    </div>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <SettingsApp />
  </StrictMode>,
);
