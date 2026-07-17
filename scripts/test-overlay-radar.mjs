/**
 * Unit tests for overlay view helpers.
 * Run: node --test scripts/test-overlay-radar.mjs
 */
import test from 'node:test';
import assert from 'node:assert/strict';

function levelToWaveBoost(level) {
  const clamped = Math.min(1, Math.max(0, level));
  return Math.pow(clamped, 0.55);
}

function viewFromSnapshot(snap) {
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

  return { visible: true, kind: 'dormant', level: 0, recordingMode: null };
}

test('levelToWaveBoost clamps and grows with level', () => {
  assert.equal(levelToWaveBoost(-1), levelToWaveBoost(0));
  assert.equal(levelToWaveBoost(2), levelToWaveBoost(1));
  assert.ok(levelToWaveBoost(1) > levelToWaveBoost(0.2));
});

test('viewFromSnapshot keeps dormant presence when idle', () => {
  assert.equal(viewFromSnapshot({ status: 'idle' }).kind, 'dormant');
  assert.equal(viewFromSnapshot({ status: 'idle' }).visible, true);
  assert.equal(viewFromSnapshot({ status: 'recording', audio: { level: 0.4 } }).kind, 'recording');
  assert.equal(viewFromSnapshot({ status: 'transcribing' }).kind, 'processing');
  assert.equal(viewFromSnapshot({ status: 'failed' }).kind, 'failed');
  assert.equal(viewFromSnapshot({ status: 'completed' }).kind, 'dormant');
});

test('recording passes through audio level', () => {
  const v = viewFromSnapshot({
    status: 'recording',
    audio: { level: 0.7, peakAmplitude: 0.2, sampleRate: 1, channels: 1, frames: 1, durationMs: 1 },
    recordingMode: 'push_to_talk',
  });
  assert.equal(v.level, 0.7);
  assert.equal(v.recordingMode, 'push_to_talk');
});
