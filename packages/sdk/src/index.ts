import axios, { type AxiosInstance } from 'axios';
import {
  AsrResponseSchema,
  HealthResponseSchema,
  RefineRequestSchema,
  RefineResponseSchema,
  type AsrResponse,
  type HealthResponse,
  type RefineRequest,
  type RefineResponse,
} from '@voice/contracts';

export type VoiceClientOptions = {
  baseUrl: string;
  getAccessToken?: () => string | null | Promise<string | null>;
};

export class VoiceApiClient {
  private readonly http: AxiosInstance;
  private readonly getAccessToken?: VoiceClientOptions['getAccessToken'];

  constructor(options: VoiceClientOptions) {
    this.getAccessToken = options.getAccessToken;
    this.http = axios.create({
      baseURL: options.baseUrl.replace(/\/$/, ''),
      timeout: 30_000,
      headers: { 'Content-Type': 'application/json' },
    });

    this.http.interceptors.request.use(async (config) => {
      const token = this.getAccessToken ? await this.getAccessToken() : null;
      if (token) {
        config.headers.Authorization = `Bearer ${token}`;
      }
      return config;
    });
  }

  async health(): Promise<HealthResponse> {
    const { data } = await this.http.get<unknown>('/v1/health');
    return HealthResponseSchema.parse(data);
  }

  async refine(input: RefineRequest): Promise<RefineResponse> {
    const payload = RefineRequestSchema.parse(input);
    const { data } = await this.http.post<unknown>('/v1/ai/refine', payload);
    return RefineResponseSchema.parse(data);
  }

  async transcribe(file: Blob, locale = 'ru'): Promise<AsrResponse> {
    const form = new FormData();
    form.append('file', file, 'dictation.wav');
    form.append('locale', locale);
    const { data } = await this.http.post<unknown>('/v1/ai/asr', form, {
      headers: { 'Content-Type': 'multipart/form-data' },
      timeout: 120_000,
    });
    return AsrResponseSchema.parse(data);
  }
}

export function createVoiceClient(options: VoiceClientOptions): VoiceApiClient {
  return new VoiceApiClient(options);
}
