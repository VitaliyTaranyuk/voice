import type { DictationSessionStatus } from '@voice/contracts';

export type CaptureStats = {
  sampleRate: number;
  channels: number;
  frames: number;
  durationMs: number;
  peakAmplitude: number;
};

export type RuntimeInfo = {
  appName: string;
  version: string;
  platform: string;
  mvpTarget: string;
  llmProvider: string;
  hotkey: string;
};

export type SessionSnapshot = {
  sessionId: string | null;
  status: DictationSessionStatus | string;
  message: string;
  audio: CaptureStats | null;
  hotkey: string;
};
