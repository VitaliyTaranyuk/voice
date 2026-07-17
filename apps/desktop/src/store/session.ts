import { create } from 'zustand';
import type { PrivacyMode } from '@voice/contracts';
import type { RuntimeInfo, SessionSnapshot } from '../types';

type SessionStore = {
  runtime: RuntimeInfo | null;
  session: SessionSnapshot | null;
  privacyMode: PrivacyMode;
  busy: boolean;
  error: string | null;
  setRuntime: (runtime: RuntimeInfo) => void;
  setSession: (session: SessionSnapshot) => void;
  setPrivacyMode: (mode: PrivacyMode) => void;
  setBusy: (busy: boolean) => void;
  setError: (error: string | null) => void;
};

export const useSessionStore = create<SessionStore>((set) => ({
  runtime: null,
  session: null,
  privacyMode: 'cloud',
  busy: false,
  error: null,
  setRuntime: (runtime) => set({ runtime }),
  setSession: (session) => set({ session }),
  setPrivacyMode: (privacyMode) => set({ privacyMode }),
  setBusy: (busy) => set({ busy }),
  setError: (error) => set({ error }),
}));
