import type { DictationSessionStatus } from '@voice/contracts';

export type RuntimeInfo = {
  appName: string;
  version: string;
  platform: string;
  mvpTarget: string;
  llmProvider: string;
};

export type SessionSnapshot = {
  sessionId: string | null;
  status: DictationSessionStatus | 'idle' | 'recording' | 'completed' | 'cancelled' | 'failed';
  message: string;
};
