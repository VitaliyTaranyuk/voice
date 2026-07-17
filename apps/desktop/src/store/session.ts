import { create } from 'zustand';
import type { PrivacyMode } from '@voice/contracts';
import type { HistoryItem, RuntimeInfo, SessionSnapshot } from '../types';

type SessionStore = {
  runtime: RuntimeInfo | null;
  session: SessionSnapshot | null;
  privacyMode: PrivacyMode;
  history: HistoryItem[];
  apiOnline: boolean | null;
  busy: boolean;
  error: string | null;
  setRuntime: (runtime: RuntimeInfo) => void;
  setSession: (session: SessionSnapshot) => void;
  setPrivacyMode: (mode: PrivacyMode) => void;
  setHistory: (history: HistoryItem[]) => void;
  setApiOnline: (online: boolean) => void;
  setBusy: (busy: boolean) => void;
  setError: (error: string | null) => void;
};

export const useSessionStore = create<SessionStore>((set) => ({
  runtime: null,
  session: null,
  privacyMode: 'cloud',
  history: [],
  apiOnline: null,
  busy: false,
  error: null,
  setRuntime: (runtime) => set({ runtime }),
  setSession: (session) => set({ session }),
  setPrivacyMode: (privacyMode) => set({ privacyMode }),
  setHistory: (history) => set({ history }),
  setApiOnline: (apiOnline) => set({ apiOnline }),
  setBusy: (busy) => set({ busy }),
  setError: (error) => set({ error }),
}));
