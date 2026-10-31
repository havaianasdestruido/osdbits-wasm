import { AppMode, AudioCueBit } from '../wasm/bridge';

export interface AudioPreferences {
  muted: boolean;
  masterVolume: number;
  ambientVolume: number;
  sfxVolume: number;
  pauseOnHidden: boolean;
}

export const DEFAULT_AUDIO_PREFERENCES: AudioPreferences = {
  muted: false,
  masterVolume: 0.7,
  ambientVolume: 0.55,
  sfxVolume: 0.75,
  pauseOnHidden: true,
};

export interface AudioEngineSnapshot {
  available: boolean;
  state: AudioContextState | 'unavailable';
  unlockedByGesture: boolean;
  muted: boolean;
  activeMode: AppMode | null;
  triggeredCueCount: number;
  degradedReason: string | null;
}

export type AudioContextConstructor = new () => AudioContext;

function clamp01(v: number, fallback: number): number {
  if (!Number.isFinite(v)) return fallback;
  return Math.min(1, Math.max(0, v));
}

function resolveAudioContextCtor(): AudioContextConstructor | null {
  if (typeof globalThis === 'undefined') return null;
  const g = globalThis as unknown as {
    AudioContext?: AudioContextConstructor;
    webkitAudioContext?: AudioContextConstructor;
  };
  return g.AudioContext ?? g.webkitAudioContext ?? null;
}

const MODE_CHORDS: Record<AppMode, { freqs: [number, number, number]; cutoff: number }> = {
  [AppMode.Opening]: { freqs: [146.83, 220.0, 329.63], cutoff: 960 },
  [AppMode.Idle]: { freqs: [146.83, 220.0, 440.0], cutoff: 680 },
  [AppMode.Illegal]: { freqs: [65.41, 92.5, 138.59], cutoff: 410 },
  [AppMode.Menu]: { freqs: [220.0, 329.63, 493.88], cutoff: 1120 },
};

/**
 * Browser-safe clean-room Web Audio synthesizer with synchronous user-gesture unlock.
 */
export class AudioEngine {
  private readonly ctor: AudioContextConstructor | null;
  private ctx: AudioContext | null = null;
  private masterGain: GainNode | null = null;
  private ambientGain: GainNode | null = null;
  private sfxGain: GainNode | null = null;
  private ambientFilter: BiquadFilterNode | null = null;
  private ambientOscs: OscillatorNode[] = [];

  private prefs: AudioPreferences = { ...DEFAULT_AUDIO_PREFERENCES };
  private activeMode: AppMode | null = null;
  private unlockedByGesture = false;
  private suspendedByVisibility = false;
  private degradedReason: string | null = null;
  private triggeredCueCount = 0;
  private disposed = false;

  private readonly onVisibilityChange = (): void => {
    if (this.disposed || !this.ctx || !this.prefs.pauseOnHidden) return;
    if (typeof document === 'undefined') return;
    if (document.visibilityState === 'hidden') {
      if (this.ctx.state === 'running') {
        this.suspendedByVisibility = true;
        void this.ctx.suspend().catch(() => undefined);
      }
    } else if (this.suspendedByVisibility && this.unlockedByGesture) {
      this.suspendedByVisibility = false;
      void this.ctx.resume().catch(() => undefined);
    }
  };

  public constructor(
    initialPrefs: Partial<AudioPreferences> = {},
    customCtor?: AudioContextConstructor | null,
  ) {
    this.ctor = customCtor !== undefined ? customCtor : resolveAudioContextCtor();
    this.setPreferences({ ...DEFAULT_AUDIO_PREFERENCES, ...initialPrefs });
    if (typeof document !== 'undefined' && typeof document.addEventListener === 'function') {
      document.addEventListener('visibilitychange', this.onVisibilityChange);
    }
  }

  /**
   * MUST be called synchronously inside a user `click` or `keydown` handler
   * BEFORE any `await` so browsers mark `AudioContext.resume()` as gesture-authorized.
   */
  public unlockFromUserGesture(): {
    startedSynchronously: boolean;
    settled: Promise<AudioEngineSnapshot>;
  } {
    if (this.disposed) {
      return {
        startedSynchronously: false,
        settled: Promise.resolve(this.snapshot()),
      };
    }

    if (!this.ctor) {
      this.degradedReason = 'Web Audio API is not supported in this browser.';
      return {
        startedSynchronously: false,
        settled: Promise.resolve(this.snapshot()),
      };
    }

    try {
      if (!this.ctx) {
        this.ctx = new this.ctor();
        this.buildAudioGraph(this.ctx);
      }

      // Synchronous silent buffer kick for WebKit/Safari gesture unlock
      if (typeof this.ctx.createBuffer === 'function' && this.masterGain) {
        const silent = this.ctx.createBuffer(1, 1, 22050);
        const src = this.ctx.createBufferSource();
        src.buffer = silent;
        src.connect(this.masterGain);
        src.start(0);
      }

      // Synchronous `resume()` call inside the user gesture call stack!
      const resumePromise =
        this.ctx.state === 'running' ? Promise.resolve() : this.ctx.resume();

      this.unlockedByGesture = true;
      this.degradedReason = null;

      const settled = resumePromise
        .then(() => {
          if (this.ctx && this.ctx.state !== 'running') {
            this.degradedReason = `AudioContext remained '${this.ctx.state}' after user gesture.`;
          }
          return this.snapshot();
        })
        .catch((err: unknown) => {
          this.degradedReason =
            err instanceof Error ? err.message : 'AudioContext.resume() rejected.';
          return this.snapshot();
        });

      return { startedSynchronously: true, settled };
    } catch (err) {
      this.degradedReason =
        err instanceof Error ? err.message : 'Failed to construct AudioContext.';
      return {
        startedSynchronously: false,
        settled: Promise.resolve(this.snapshot()),
      };
    }
  }

  private buildAudioGraph(ctx: AudioContext): void {
    const master = ctx.createGain();
    const ambient = ctx.createGain();
    const sfx = ctx.createGain();
    const filter = ctx.createBiquadFilter();

    filter.type = 'lowpass';
    filter.frequency.value = 900;
    filter.Q.value = 0.9;

    master.connect(ctx.destination);
    filter.connect(ambient);
    ambient.connect(master);
    sfx.connect(master);

    this.masterGain = master;
    this.ambientGain = ambient;
    this.sfxGain = sfx;
    this.ambientFilter = filter;

    const chord = MODE_CHORDS[this.activeMode ?? AppMode.Opening];
    filter.frequency.value = chord.cutoff;

    const types: OscillatorType[] = ['sine', 'triangle', 'sine'];
    const gains = [0.22, 0.14, 0.09];
    this.ambientOscs = chord.freqs.map((freq, idx) => {
      const osc = ctx.createOscillator();
      const voiceGain = ctx.createGain();
      osc.type = types[idx] ?? 'sine';
      osc.frequency.value = freq;
      voiceGain.gain.value = gains[idx] ?? 0.1;
      osc.connect(voiceGain);
      voiceGain.connect(filter);
      osc.start();
      return osc;
    });

    this.applyGains();
  }

  private applyGains(): void {
    if (!this.masterGain || !this.ambientGain || !this.sfxGain) return;
    const masterTarget = this.prefs.muted ? 0 : this.prefs.masterVolume;
    this.masterGain.gain.value = masterTarget;
    this.ambientGain.gain.value = this.prefs.ambientVolume * 0.45;
    this.sfxGain.gain.value = this.prefs.sfxVolume;
  }

  public setPreferences(next: Partial<AudioPreferences>): void {
    this.prefs = {
      muted: next.muted ?? this.prefs.muted,
      masterVolume: clamp01(next.masterVolume ?? this.prefs.masterVolume, 0.7),
      ambientVolume: clamp01(next.ambientVolume ?? this.prefs.ambientVolume, 0.55),
      sfxVolume: clamp01(next.sfxVolume ?? this.prefs.sfxVolume, 0.75),
      pauseOnHidden: next.pauseOnHidden ?? this.prefs.pauseOnHidden,
    };
    this.applyGains();
  }

  public getPreferences(): Readonly<AudioPreferences> {
    return this.prefs;
  }

  public setSceneMode(mode: AppMode): void {
    this.activeMode = mode;
    if (!this.ctx || !this.ambientFilter || this.ambientOscs.length === 0) return;

    const chord = MODE_CHORDS[mode];
    const now = this.ctx.currentTime;
    if (typeof this.ambientFilter.frequency.setTargetAtTime === 'function') {
      this.ambientFilter.frequency.setTargetAtTime(chord.cutoff, now, 0.25);
    } else {
      this.ambientFilter.frequency.value = chord.cutoff;
    }

    chord.freqs.forEach((freq, idx) => {
      const osc = this.ambientOscs[idx];
      if (!osc) return;
      if (typeof osc.frequency.setTargetAtTime === 'function') {
        osc.frequency.setTargetAtTime(freq, now, 0.18);
      } else {
        osc.frequency.value = freq;
      }
    });
  }

  /**
   * Play clean-room synthesized chimes corresponding to WASM `telemetry.audioCues` bitflags.
   */
  public triggerCues(cueMask: number): void {
    if (cueMask === 0 || this.disposed) return;
    if (!this.ctx || !this.sfxGain || this.prefs.muted || this.ctx.state !== 'running') {
      return;
    }

    if ((cueMask & AudioCueBit.CursorTick) !== 0) {
      this.playTone(1318.5, 1760.0, 0.055, 0.14, 'sine');
    }
    if ((cueMask & AudioCueBit.Confirm) !== 0) {
      this.playTone(880.0, 1318.5, 0.17, 0.2, 'triangle');
    }
    if ((cueMask & AudioCueBit.Cancel) !== 0) {
      this.playTone(783.99, 523.25, 0.16, 0.18, 'sine');
    }
    if ((cueMask & AudioCueBit.TransitionIn) !== 0) {
      this.playTone(587.33, 1174.66, 0.28, 0.16, 'triangle');
    }
    if ((cueMask & AudioCueBit.TransitionOut) !== 0) {
      this.playTone(987.77, 659.25, 0.24, 0.15, 'sine');
    }
  }

  private playTone(
    startHz: number,
    endHz: number,
    durationSec: number,
    peakGain: number,
    type: OscillatorType,
  ): void {
    if (!this.ctx || !this.sfxGain) return;
    const now = this.ctx.currentTime;
    const osc = this.ctx.createOscillator();
    const env = this.ctx.createGain();
    osc.type = type;
    osc.frequency.value = startHz;
    if (typeof osc.frequency.exponentialRampToValueAtTime === 'function') {
      osc.frequency.setValueAtTime(startHz, now);
      osc.frequency.exponentialRampToValueAtTime(Math.max(20, endHz), now + durationSec);
    }
    env.gain.value = peakGain;
    if (typeof env.gain.exponentialRampToValueAtTime === 'function') {
      env.gain.setValueAtTime(peakGain, now);
      env.gain.exponentialRampToValueAtTime(0.0008, now + durationSec);
    }
    osc.connect(env);
    env.connect(this.sfxGain);
    osc.start(now);
    osc.stop(now + durationSec + 0.01);
    this.triggeredCueCount += 1;
  }

  public snapshot(): AudioEngineSnapshot {
    return {
      available: this.ctor !== null,
      state: this.ctx ? this.ctx.state : 'unavailable',
      unlockedByGesture: this.unlockedByGesture,
      muted: this.prefs.muted,
      activeMode: this.activeMode,
      triggeredCueCount: this.triggeredCueCount,
      degradedReason: this.degradedReason,
    };
  }

  public dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    if (typeof document !== 'undefined' && typeof document.removeEventListener === 'function') {
      document.removeEventListener('visibilitychange', this.onVisibilityChange);
    }
    for (const osc of this.ambientOscs) {
      try {
        osc.stop();
      } catch {
        // Ignore already-stopped oscillators
      }
    }
    this.ambientOscs = [];
    if (this.ctx && this.ctx.state !== 'closed') {
      void this.ctx.close().catch(() => undefined);
    }
    this.ctx = null;
  }
}
