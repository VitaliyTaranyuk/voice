import type {
  AppContext,
  DictationSessionStatus,
  HotkeyMode,
  PrivacyMode,
} from '@voice/contracts';

export type DictationSession = {
  id: string;
  startedAt: string;
  status: DictationSessionStatus;
  privacyMode: PrivacyMode;
  hotkeyMode: HotkeyMode;
  appContext?: AppContext;
  rawText?: string;
  refinedText?: string;
  errorMessage?: string;
};

export type DictionaryCategory =
  | 'names'
  | 'companies'
  | 'programming'
  | 'apis'
  | 'cli'
  | 'brands';

export type DictionaryEntry = {
  id: string;
  term: string;
  pronunciations: string[];
  priority: number;
  category: DictionaryCategory;
  notes?: string;
};

export type InstructionScope = 'global' | 'app' | 'context';

export type Instruction = {
  id: string;
  scope: InstructionScope;
  scopeKey?: string;
  body: string;
  priority: number;
  enabled: boolean;
};

export type HistoryItem = {
  id: string;
  sessionId: string;
  text: string;
  rawText?: string;
  appId?: string;
  createdAt: string;
  favorite: boolean;
};

export type UserPreferences = {
  privacyMode: PrivacyMode;
  hotkeyMode: HotkeyMode;
  asrProviderId: string;
  llmProviderId: 'deepseek';
  launchOnStartup: boolean;
};
