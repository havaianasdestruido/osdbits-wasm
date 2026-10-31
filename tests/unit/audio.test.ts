/**
 * Audio lifecycle tests.
 *
 * The critical contract is the browser activation rule: `AudioContext.resume()` must be started
 * synchronously inside the click handler, before any `await` yields the user gesture. These
 * tests use a recording mock context so the ordering is assertable.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  AudioEngine,
  BGM_SLOT_BOOTS,
  BGM_SLOT_TNNLS,
  SE_CONFIRM,
  SND_CMD_BGM_PLAY,
  SND_CMD_BGM_STOP,
  SND_CMD_REVERB_DEPTH,
  SND_CMD_SE_PLAY,
} from '../../src/audio/audio-engine';

interface RecordingContext {
  calls: string[];
  state: string;
  resume: () => Promise<void>;
  suspend: () => Promise<void>;
  close: () => Promise<void>;
  createGain: () => unknown;
  createOscillator: () => unknown;
  createBiquadFilter: () => unknown;
  createConvolver: () => unknown;
  createDynamicsCompressor: () => unknown;
  createBuffer: () => unknown;
  destination: unknown;
  currentTime: number;
  sampleRate: number;
}

function mockAudioContext(calls: string[]): RecordingContext {
  const param = { value: 0, setValueAtTime: vi.fn(), setTargetAtTime: vi.fn(), linearRampToValueAtTime: vi.fn(), exponentialRampToValueAtTime: vi.fn(), cancelScheduledValues: vi.fn() };
  const node = (kind: string) => ({
    kind,
    gain: param,
    frequency: param,
    detune: param,
    Q: param,
    threshold: param,
    ratio: param,
    attack: param,
    release: param,
    type: 'sine',
    buffer: null,
    connect: vi.fn(),
    disconnect: vi.fn(),
    start: vi.fn(() => calls.push(`${kind}.start`)),
    stop: vi.fn(),
  });
  return {
    calls,
    state: 'suspended',
    resume: () => {
      calls.push('resume');
      return Promise.resolve();
    },
    suspend: () => Promise.resolve(),
    close: () => Promise.resolve(),
    createGain: () => node('gain'),
    createOscillator: () => node('osc'),
    createBiquadFilter: () => node('filter'),
    createConvolver: () => node('convolver'),
    createDynamicsCompressor: () => node('compressor'),
    createBuffer: () => ({ getChannelData: () => new Float32Array(16) }),
    destination: {},
    currentTime: 0,
    sampleRate: 48000,
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('audio engine', () => {
  it('resumes the context synchronously inside unlock()', () => {
    const calls: string[] = [];
    const Ctor = vi.fn(() => mockAudioContext(calls));
    vi.stubGlobal('window', { AudioContext: Ctor });

    const engine = new AudioEngine();
    const result = engine.unlock();
    expect(result.ok).toBe(true);
    // `resume` must be invoked before unlock() returns: no await happened in between.
    expect(calls).toContain('resume');
    expect(engine.status().unlocked).toBe(true);
  });

  it('does not create an AudioContext before the user gesture', () => {
    const Ctor = vi.fn();
    vi.stubGlobal('window', { AudioContext: Ctor });
    const engine = new AudioEngine();
    expect(Ctor).not.toHaveBeenCalled();
    expect(engine.status().unlocked).toBe(false);
    expect(engine.status().contextState).toBe('none');
  });

  it('reports a graceful failure when Web Audio is missing', () => {
    vi.stubGlobal('window', {});
    const engine = new AudioEngine();
    const result = engine.unlock();
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/not available/i);
    // Still safe to process messages.
    expect(() => engine.processMessages([{ msg: SND_CMD_BGM_PLAY, a: 0, b: 0, c: 0 }])).not.toThrow();
  });

  it('routes BGM play/stop and sound effects without leaking beds', () => {
    const calls: string[] = [];
    vi.stubGlobal('window', { AudioContext: vi.fn(() => mockAudioContext(calls)) });
    const engine = new AudioEngine();
    engine.unlock();

    engine.processMessages([
      { msg: SND_CMD_BGM_PLAY, a: BGM_SLOT_BOOTS, b: 0, c: 0 },
      { msg: SND_CMD_BGM_PLAY, a: BGM_SLOT_TNNLS, b: 0, c: 0 },
      { msg: SND_CMD_SE_PLAY, a: 1, b: SE_CONFIRM, c: 0 },
      { msg: SND_CMD_REVERB_DEPTH, a: 0, b: 0x8000, c: 0x8000 },
    ]);
    expect(engine.status().activeBeds.sort()).toEqual(['boots', 'tnnls']);

    // Re-playing the same slot must not duplicate the bed.
    engine.processMessages([{ msg: SND_CMD_BGM_PLAY, a: BGM_SLOT_BOOTS, b: 0, c: 0 }]);
    expect(engine.status().activeBeds.filter((b) => b === 'boots')).toHaveLength(1);

    engine.processMessages([{ msg: SND_CMD_BGM_STOP, a: BGM_SLOT_BOOTS, b: 0, c: 15 }]);
    expect(engine.status().activeBeds).toEqual(['tnnls']);

    engine.reset();
    expect(engine.status().activeBeds).toEqual([]);
  });

  it('applies mute and volume without touching the rendering path', () => {
    vi.stubGlobal('window', { AudioContext: vi.fn(() => mockAudioContext([])) });
    const engine = new AudioEngine();
    engine.unlock();
    engine.setMasterVolume(2);
    expect(engine.status().masterVolume).toBe(1);
    engine.setMasterVolume(-1);
    expect(engine.status().masterVolume).toBe(0);
    engine.setMuted(true);
    expect(engine.status().muted).toBe(true);
    engine.setBgmVolume(0.25);
    engine.setSeVolume(0.5);
    expect(engine.status().muted).toBe(true);
  });
});
