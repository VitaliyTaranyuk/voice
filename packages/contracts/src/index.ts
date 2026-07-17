import { z } from 'zod';

export const PrivacyModeSchema = z.enum(['local', 'hybrid', 'cloud']);
export type PrivacyMode = z.infer<typeof PrivacyModeSchema>;

export const AppCategorySchema = z.enum([
  'ide',
  'chat',
  'email',
  'browser',
  'docs',
  'other',
]);
export type AppCategory = z.infer<typeof AppCategorySchema>;

export const HotkeyModeSchema = z.enum(['push_to_talk', 'toggle', 'hands_free']);
export type HotkeyMode = z.infer<typeof HotkeyModeSchema>;

export const AppContextSchema = z.object({
  appId: z.string().min(1),
  appCategory: AppCategorySchema,
  windowTitle: z.string().optional(),
  processName: z.string().optional(),
});
export type AppContext = z.infer<typeof AppContextSchema>;

export const ApiErrorSchema = z.object({
  code: z.string(),
  message: z.string(),
  retryable: z.boolean().default(false),
  request_id: z.string().optional(),
});
export type ApiError = z.infer<typeof ApiErrorSchema>;

export const HealthResponseSchema = z.object({
  status: z.literal('ok'),
  service: z.string(),
  version: z.string(),
});
export type HealthResponse = z.infer<typeof HealthResponseSchema>;

export const RefineRequestSchema = z.object({
  rawTranscript: z.string().min(1),
  locale: z.string().default('ru-RU'),
  privacyMode: PrivacyModeSchema,
  appContext: AppContextSchema.optional(),
  instructions: z.array(z.string()).default([]),
  dictionaryHints: z.array(z.string()).default([]),
});
export type RefineRequest = z.infer<typeof RefineRequestSchema>;

export const RefineResponseSchema = z.object({
  text: z.string(),
  confidence: z.number().min(0).max(1).optional(),
  applied_rules: z.array(z.string()).default([]),
  warnings: z.array(z.string()).default([]),
  provider: z.literal('deepseek'),
});
export type RefineResponse = z.infer<typeof RefineResponseSchema>;

export const DictationSessionStatusSchema = z.enum([
  'idle',
  'recording',
  'transcribing',
  'refining',
  'injecting',
  'completed',
  'failed',
  'cancelled',
]);
export type DictationSessionStatus = z.infer<typeof DictationSessionStatusSchema>;
