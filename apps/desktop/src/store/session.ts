import { create } from 'zustand';
import type { RuntimeInfo, SessionSnapshot } from '../types';

type SessionStore = {
  runtime: RuntimeInfo | null;
  session: SessionSnapshot | null;
  lastText: string | null;
  apiOnline: boolean | null;
  error: string | null;
  setRuntime: (runtime: RuntimeInfo) => void;
  setSession: (session: SessionSnapshot) => void;
  setLastText: (lastText: string | null) => void;
  setApiOnline: (online: boolean) => void;
  setError: (error: string | null) => void;
};

export const useSessionStore = create<SessionStore>((set) => ({
  runtime: null,
  session: null,
  lastText: null,
  apiOnline: null,
  error: null,
  setRuntime: (runtime) => set({ runtime }),
  setSession: (session) => set({ session }),
  setLastText: (lastText) => set({ lastText }),
  setApiOnline: (apiOnline) => set({ apiOnline }),
  setError: (error) => set({ error }),
}));
