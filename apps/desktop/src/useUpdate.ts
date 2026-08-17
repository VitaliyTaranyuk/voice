import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { check, type Update } from '@tauri-apps/plugin-updater';

/**
 * Update state machine. Deliberately notification-only: the user decides when to
 * install. A dictation utility that blocks work until it has restarted itself is
 * worse than one release behind.
 */
export type UpdateStage =
  | { kind: 'idle' }
  | { kind: 'available'; version: string }
  | { kind: 'downloading'; version: string; percent: number | null }
  | { kind: 'installing'; version: string }
  | { kind: 'failed'; message: string };

export function useUpdate() {
  const [stage, setStage] = useState<UpdateStage>({ kind: 'idle' });
  const [update, setUpdate] = useState<Update | null>(null);

  // Startup check only, and silent on failure: no network, a blocked endpoint or
  // a rejected signature must not greet the user with an error they did not ask
  // for. Nothing is offered, and that is the safe default (W-06).
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const found = await check();
        if (cancelled || !found) return;
        setUpdate(found);
        setStage({ kind: 'available', version: found.version });
      } catch {
        /* offline or endpoint unreachable — stay silent */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const install = useCallback(async () => {
    if (!update) return;
    setStage({ kind: 'downloading', version: update.version, percent: null });

    let total = 0;
    let received = 0;
    try {
      // Download and install are two calls rather than downloadAndInstall(), so
      // that stopping the API happens between them: dictation keeps working for
      // the whole download, and a download that fails leaves nothing to restore.
      await update.download((event) => {
        if (event.event === 'Started') {
          total = event.data.contentLength ?? 0;
          received = 0;
        } else if (event.event === 'Progress') {
          received += event.data.chunkLength;
        }
        setStage({
          kind: 'downloading',
          version: update.version,
          // Content-Length is advisory: without it a percentage would be a lie,
          // so the UI shows an indeterminate state instead of a made-up number.
          percent: total > 0 ? Math.min(100, Math.round((received / total) * 100)) : null,
        });
      });

      // The sidecar holds resources\voice-api\_internal\*.pyd open and NSIS
      // cannot overwrite a file in use — that is how the 0.1.3 update stopped
      // halfway, with the main binary replaced and the sidecar left at 0.1.2.
      await invoke('stop_local_api');

      setStage({ kind: 'installing', version: update.version });
      // Nothing after this line runs. install() launches the installer and ends
      // the process with exit(0); the installer starts Voice again itself,
      // because installMode "passive" passes it /R.
      await update.install();
    } catch (err) {
      // Here the user did ask, so the failure has to be visible.
      setStage({ kind: 'failed', message: err instanceof Error ? err.message : String(err) });
    }
  }, [update]);

  return { stage, install };
}
