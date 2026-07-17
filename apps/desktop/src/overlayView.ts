import type { RecordingMode, SessionSnapshot } from './types';

export type OverlayKind = 'dormant' | 'recording' | 'processing' | 'failed';

export type OverlayView = {
  visible: boolean;
  kind: OverlayKind;
  level: number;
  recordingMode: RecordingMode | null;
};

/** Below this (after gain), treat as silence. */
export const SILENCE_THRESHOLD = 0.02;

/** Mic peaks are often quiet — boost before mapping to UI. */
const LEVEL_GAIN = 7;

export function viewFromSnapshot(snap: SessionSnapshot | null): OverlayView {
  const status = snap?.status ?? 'idle';
  const level = snap?.audio?.level ?? snap?.audio?.peakAmplitude ?? 0;
  const recordingMode = snap?.recordingMode ?? null;

  if (status === 'recording') {
    return { visible: true, kind: 'recording', level, recordingMode };
  }
  if (status === 'transcribing' || status === 'refining' || status === 'injecting') {
    return { visible: true, kind: 'processing', level: 0, recordingMode: null };
  }
  if (status === 'failed') {
    return { visible: true, kind: 'failed', level: 0, recordingMode: null };
  }

  // idle / cancelled / completed — always-on presence mark (Aquavos-style).
  return { visible: true, kind: 'dormant', level: 0, recordingMode: null };
}

export function recordingModeClass(mode: RecordingMode | null): string {
  if (mode === 'toggle') return 'is-toggle';
  if (mode === 'push_to_talk') return 'is-ptt';
  return '';
}

/** Map mic level to wave energy (0..1); silence → 0. */
export function levelToWaveBoost(level: number): number {
  const boosted = Math.min(1, Math.max(0, level) * LEVEL_GAIN);
  if (boosted < SILENCE_THRESHOLD) return 0;
  const remapped = (boosted - SILENCE_THRESHOLD) / (1 - SILENCE_THRESHOLD);
  return Math.pow(remapped, 0.45);
}

export type WaveStyle = {
  transform: string;
  opacity: number;
};

/** Per-ring scale/opacity for a given energy (fits ~120px overlay). */
export function waveStylesForLevel(level: number): [WaveStyle, WaveStyle, WaveStyle] {
  const l = Math.min(1, Math.max(0, level));
  // Idle listening: faint halo so orb ≠ static app icon.
  const idle = l < 0.03;
  if (idle) {
    return [
      { transform: 'translate(-50%, -50%) scale(1.22)', opacity: 0.1 },
      { transform: 'translate(-50%, -50%) scale(1.4)', opacity: 0.05 },
      { transform: 'translate(-50%, -50%) scale(1.55)', opacity: 0.025 },
    ];
  }
  return [
    {
      transform: `translate(-50%, -50%) scale(${(1.2 + l * 0.95).toFixed(3)})`,
      opacity: 0.2 + l * 0.35,
    },
    {
      transform: `translate(-50%, -50%) scale(${(1.35 + l * 1.15).toFixed(3)})`,
      opacity: 0.14 + l * 0.28,
    },
    {
      transform: `translate(-50%, -50%) scale(${(1.5 + l * 1.35).toFixed(3)})`,
      opacity: 0.08 + l * 0.2,
    },
  ];
}
