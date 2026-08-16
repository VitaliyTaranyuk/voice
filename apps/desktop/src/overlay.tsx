import { StrictMode, useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { SessionSnapshot } from './types';
import {
  levelToWaveBoost,
  recordingModeClass,
  viewFromSnapshot,
  waveStylesForLevel,
  type OverlayView,
} from './overlayView';
import './overlay.css';

/** Dim idle → bright when voice energy rises (smoothed level 0..1). */
function glowFromLevel(level: number): {
  glow: number;
  alpha: number;
  blur: number;
  opacity: number;
} {
  const l = Math.min(1, Math.max(0, level));
  return {
    // Idle readable but quieter; voice brings full opacity + stronger halo.
    glow: 0.72 + l * 0.55,
    alpha: 0.22 + l * 0.45,
    blur: 8 + l * 12,
    opacity: 0.68 + l * 0.32,
  };
}

/** Soft 6-petal flower / droplet-star (no sharp corners). */
function OrbCore() {
  return (
    <svg className="orb-core" viewBox="0 0 64 64" aria-hidden>
      <defs>
        <radialGradient id="orbGrad" cx="36%" cy="28%" r="72%">
          <stop offset="0%" stopColor="#ffd4b0" />
          <stop offset="35%" stopColor="#f0a06a" />
          <stop offset="75%" stopColor="#e8905c" />
          <stop offset="100%" stopColor="#c46a3a" />
        </radialGradient>
        {/* Darker / glassier fill for always-on dormant mark */}
        <radialGradient id="orbGradDormant" cx="34%" cy="26%" r="70%">
          <stop offset="0%" stopColor="#d4a888" stopOpacity="0.85" />
          <stop offset="40%" stopColor="#a87050" stopOpacity="0.75" />
          <stop offset="100%" stopColor="#6a4030" stopOpacity="0.9" />
        </radialGradient>
      </defs>
      <path
        fill="url(#orbGrad)"
        d="M32.00,10.00 L33.14,10.19 L34.23,10.75 L35.23,11.61 L36.11,12.69 L36.85,13.88 L37.50,15.09 L38.06,16.21 L38.60,17.17 L39.17,17.92 L39.83,18.44 L40.61,18.75 L41.54,18.86 L42.65,18.85 L43.90,18.78 L45.26,18.74 L46.67,18.79 L48.05,19.01 L49.29,19.44 L50.32,20.11 L51.05,21.00 L51.46,22.09 L51.52,23.31 L51.28,24.60 L50.78,25.90 L50.12,27.15 L49.39,28.30 L48.71,29.35 L48.15,30.30 L47.78,31.17 L47.66,32.00 L47.78,32.83 L48.15,33.70 L48.71,34.65 L49.39,35.70 L50.12,36.85 L50.78,38.10 L51.28,39.40 L51.52,40.69 L51.46,41.91 L51.05,43.00 L50.32,43.89 L49.29,44.56 L48.05,44.99 L46.67,45.21 L45.26,45.26 L43.90,45.22 L42.65,45.15 L41.54,45.14 L40.61,45.25 L39.83,45.56 L39.17,46.08 L38.60,46.83 L38.06,47.79 L37.50,48.91 L36.85,50.12 L36.11,51.31 L35.23,52.39 L34.23,53.25 L33.14,53.81 L32.00,54.00 L30.86,53.81 L29.77,53.25 L28.77,52.39 L27.89,51.31 L27.15,50.12 L26.50,48.91 L25.94,47.79 L25.40,46.83 L24.83,46.08 L24.17,45.56 L23.39,45.25 L22.46,45.14 L21.35,45.15 L20.10,45.22 L18.74,45.26 L17.33,45.21 L15.95,44.99 L14.71,44.56 L13.68,43.89 L12.95,43.00 L12.54,41.91 L12.48,40.69 L12.72,39.40 L13.22,38.10 L13.88,36.85 L14.61,35.70 L15.29,34.65 L15.85,33.70 L16.22,32.83 L16.34,32.00 L16.22,31.17 L15.85,30.30 L15.29,29.35 L14.61,28.30 L13.88,27.15 L13.22,25.90 L12.72,24.60 L12.48,23.31 L12.54,22.09 L12.95,21.00 L13.68,20.11 L14.71,19.44 L15.95,19.01 L17.33,18.79 L18.74,18.74 L20.10,18.78 L21.35,18.85 L22.46,18.86 L23.39,18.75 L24.17,18.44 L24.83,17.92 L25.40,17.17 L25.94,16.21 L26.50,15.09 L27.15,13.88 L27.89,12.69 L28.77,11.61 L29.77,10.75 L30.86,10.19 Z"
      />
    </svg>
  );
}

function OverlayApp() {
  const [view, setView] = useState<OverlayView>({
    visible: true,
    kind: 'dormant',
    level: 0,
    recordingMode: null,
  });
  const [silent, setSilent] = useState(true);
  const targetLevelRef = useRef(0);
  const smoothedRef = useRef(0);
  const silentRef = useRef(true);
  const rafRef = useRef(0);
  const recordingRef = useRef(false);
  const dormantRef = useRef(true);
  const waveRefs = useRef<(HTMLSpanElement | null)[]>([null, null, null]);
  const orbRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    void (async () => {
      try {
        const snap = await invoke<SessionSnapshot>('get_session_status');
        setView(viewFromSnapshot(snap));
      } catch {
        /* ignore */
      }

      unlisten = await listen<SessionSnapshot>('dictation://status', (event) => {
        setView(viewFromSnapshot(event.payload));
      });
    })();

    const poll = window.setInterval(() => {
      void invoke<SessionSnapshot>('get_session_status')
        .then((snap) => setView(viewFromSnapshot(snap)))
        .catch(() => undefined);
    }, 40);

    return () => {
      unlisten?.();
      window.clearInterval(poll);
    };
  }, []);

  useEffect(() => {
    recordingRef.current = view.visible && view.kind === 'recording';
    dormantRef.current = view.kind === 'dormant';
    targetLevelRef.current = recordingRef.current ? levelToWaveBoost(view.level) : 0;
  }, [view]);

  useEffect(() => {
    const tick = () => {
      const target = targetLevelRef.current;
      const prev = smoothedRef.current;
      const alpha = target > prev ? 0.18 : 0.08;
      const next = prev + (target - prev) * alpha;
      smoothedRef.current = next < 0.003 ? 0 : next;
      const level = recordingRef.current ? smoothedRef.current : 0;

      // Итерируем сам кортеж: длина берётся из данных, а не дублируется константой,
      // и элемент приходит типизированным (при индексации по number
      // noUncheckedIndexedAccess дал бы WaveStyle | undefined).
      waveStylesForLevel(level).forEach((wave, i) => {
        const el = waveRefs.current[i];
        if (!el) return;
        if (!recordingRef.current) {
          el.style.opacity = '0';
          return;
        }
        el.style.transform = wave.transform;
        el.style.opacity = String(wave.opacity);
      });

      // Dormant uses CSS defaults; recording glow follows voice energy.
      const orb = orbRef.current;
      if (orb && !dormantRef.current) {
        const g = glowFromLevel(recordingRef.current ? level : 0);
        orb.style.setProperty('--glow', g.glow.toFixed(3));
        orb.style.setProperty('--glow-a', g.alpha.toFixed(3));
        orb.style.setProperty('--glow-blur', `${g.blur.toFixed(1)}px`);
        orb.style.setProperty('--core-opacity', g.opacity.toFixed(3));
      } else if (orb && dormantRef.current) {
        orb.style.removeProperty('--glow');
        orb.style.removeProperty('--glow-a');
        orb.style.removeProperty('--glow-blur');
        orb.style.removeProperty('--core-opacity');
      }

      const nextSilent = !recordingRef.current || level < 0.05;
      if (nextSilent !== silentRef.current) {
        silentRef.current = nextSilent;
        setSilent(nextSilent);
      }
      rafRef.current = window.requestAnimationFrame(tick);
    };

    rafRef.current = window.requestAnimationFrame(tick);
    return () => {
      window.cancelAnimationFrame(rafRef.current);
    };
  }, []);

  if (!view.visible) {
    return null;
  }

  const modeClass = view.kind === 'recording' ? recordingModeClass(view.recordingMode) : '';

  return (
    <div className="orb-stage">
      <div
        ref={orbRef}
        className={[
          'orb',
          `is-${view.kind}`,
          modeClass,
          view.kind === 'recording' && silent ? 'is-silent' : '',
        ]
          .filter(Boolean)
          .join(' ')}
        aria-hidden
      >
        <span
          className="orb-wave"
          ref={(el) => {
            waveRefs.current[0] = el;
          }}
        />
        <span
          className="orb-wave"
          ref={(el) => {
            waveRefs.current[1] = el;
          }}
        />
        <span
          className="orb-wave"
          ref={(el) => {
            waveRefs.current[2] = el;
          }}
        />
        <OrbCore />
      </div>
    </div>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <OverlayApp />
  </StrictMode>,
);
