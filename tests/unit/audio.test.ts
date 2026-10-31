import { describe, expect, it, vi } from 'vitest';

import {
  type AudioContextConstructor,
  AudioEngine,
} from '../../src/audio/audioEngine';
import { AppMode, AudioCueBit } from '../../src/wasm/bridge';

function createFakeAudioContextCtor(options?: {
  throwOnConstruct?: boolean;
  rejectResume?: boolean;
  staySuspended?: boolean;
}): {
  ctor: AudioContextConstructor;
  wasResumeCalledSynchronously: () => boolean;
} {
  let resumeSync = false;

  class FakeAudioContext {
    public state: AudioContextState = 'suspended';
    public currentTime = 0;
    public destination = {} as AudioDestinationNode;

    public constructor() {
      if (options?.throwOnConstruct) {
        throw new Error('Blocked by sandbox policy');
      }
    }

    public createGain(): GainNode {
      return {
        gain: {
          value: 1,
          setValueAtTime: vi.fn(),
          exponentialRampToValueAtTime: vi.fn(),
        },
        connect: vi.fn(),
      } as unknown as GainNode;
    }

    public createBiquadFilter(): BiquadFilterNode {
      return {
        type: 'lowpass',
        frequency: { value: 900, setTargetAtTime: vi.fn() },
        Q: { value: 1 },
        connect: vi.fn(),
      } as unknown as BiquadFilterNode;
    }

    public createOscillator(): OscillatorNode {
      return {
        type: 'sine',
        frequency: {
          value: 220,
          setValueAtTime: vi.fn(),
          setTargetAtTime: vi.fn(),
          exponentialRampToValueAtTime: vi.fn(),
        },
        connect: vi.fn(),
        start: vi.fn(),
        stop: vi.fn(),
      } as unknown as OscillatorNode;
    }

    public createBuffer(): AudioBuffer {
      return {} as AudioBuffer;
    }

    public createBufferSource(): AudioBufferSourceNode {
      return {
        buffer: null,
        connect: vi.fn(),
        start: vi.fn(),
      } as unknown as AudioBufferSourceNode;
    }

    public resume(): Promise<void> {
      resumeSync = true;
      if (options?.rejectResume) {
        return Promise.reject(new Error('NotAllowedError'));
      }
      if (!options?.staySuspended) {
        this.state = 'running';
      }
      return Promise.resolve();
    }

    public suspend(): Promise<void> {
      this.state = 'suspended';
      return Promise.resolve();
    }

    public close(): Promise<void> {
      this.state = 'closed';
      return Promise.resolve();
    }
  }

  return {
    ctor: FakeAudioContext as unknown as AudioContextConstructor,
    wasResumeCalledSynchronously: () => resumeSync,
  };
}

describe('Phase 4 AudioEngine unit suite (P04-001)', () => {
  it('invokes AudioContext constructor and resume() synchronously inside gesture unlock', async () => {
    const { ctor, wasResumeCalledSynchronously } = createFakeAudioContextCtor();
    const audio = new AudioEngine({}, ctor);

    expect(wasResumeCalledSynchronously()).toBe(false);
    const { startedSynchronously, settled } = audio.unlockFromUserGesture();
    // Crucial browser autoplay invariant: resume() already ran BEFORE awaiting `settled`
    expect(startedSynchronously).toBe(true);
    expect(wasResumeCalledSynchronously()).toBe(true);

    const snap = await settled;
    expect(snap.state).toBe('running');
    expect(snap.unlockedByGesture).toBe(true);
    expect(snap.degradedReason).toBeNull();

    audio.setSceneMode(AppMode.Menu);
    audio.triggerCues(AudioCueBit.CursorTick | AudioCueBit.Confirm);
    expect(audio.snapshot().triggeredCueCount).toBe(2);

    // Muting suppresses SFX triggers
    audio.setPreferences({ muted: true });
    audio.triggerCues(AudioCueBit.Cancel);
    expect(audio.snapshot().triggeredCueCount).toBe(2);

    audio.dispose();
  });

  it('degrades silently without throwing when AudioContext is missing, throws, or rejects resume()', async () => {
    const missing = new AudioEngine({}, null);
    const missingRes = await missing.unlockFromUserGesture().settled;
    expect(missingRes.available).toBe(false);
    expect(missingRes.state).toBe('unavailable');
    expect(missingRes.degradedReason).toContain('not supported');

    const throwing = new AudioEngine(
      {},
      createFakeAudioContextCtor({ throwOnConstruct: true }).ctor,
    );
    const throwingRes = await throwing.unlockFromUserGesture().settled;
    expect(throwingRes.degradedReason).toContain('Blocked');

    const rejected = new AudioEngine(
      {},
      createFakeAudioContextCtor({ rejectResume: true }).ctor,
    );
    const rejectedRes = await rejected.unlockFromUserGesture().settled;
    expect(rejectedRes.degradedReason).toContain('NotAllowedError');
  });
});
