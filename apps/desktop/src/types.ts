import type { DictationSessionStatus } from '@voice/contracts';

export type CaptureStats = {
  sampleRate: number;
  channels: number;
  frames: number;
  durationMs: number;
  peakAmplitude: number;
  /** Rolling level for live meters (0..1). */
  level?: number;
};

export type AppContext = {
  appId: string;
  appCategory: string;
  windowTitle?: string | null;
  processName?: string | null;
};

export type RuntimeInfo = {
  appName: string;
  version: string;
  platform: string;
  mvpTarget: string;
  llmProvider: string;
  hotkey: string;
  apiBaseUrl: string;
};

export type RecordingMode = 'push_to_talk' | 'toggle';

export type SessionSnapshot = {
  sessionId: string | null;
  status: DictationSessionStatus | string;
  message: string;
  audio: CaptureStats | null;
  hotkey: string;
  rawText?: string | null;
  finalText?: string | null;
  appContext?: AppContext | null;
  recordingMode?: RecordingMode | null;
  canInsert?: boolean | null;
};

/**
 * Whether a provider key is stored, never the key itself — the value stays in the
 * Windows Credential Manager and is read only by Rust when it spawns the API.
 */
export type ApiKeyStatus = {
  provider: string;
  configured: boolean;
};

export type ApiKeySaveResult = {
  keys: ApiKeyStatus[];
  /** Set when the key was stored but the running API has not picked it up yet. */
  notice: string | null;
};

export type HistoryItem = {
  id: string;
  sessionId: string;
  text: string;
  rawText?: string | null;
  appId?: string | null;
  createdAt: string;
  favorite: boolean;
};
