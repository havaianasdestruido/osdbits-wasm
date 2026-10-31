/**
 * Browser-safe, clean-room procedural Web Audio soundscape & UI foley engine (`P08-001`).
 *
 * - Emits ZERO sound and creates ZERO running nodes prior to explicit user gesture.
 * - `activateFromUserGesture()` invokes `AudioContext.resume()` synchronously inside the
 *   user's single `Start OSDSYS` click/keydown handler before any `await`.
 * - Contains ZERO proprietary Sony `SNDIMAGE` samples; synthesizes original early-2000s
 *   ambient harmonic pads, wind shimmer, and crystalline menu foley via Web Audio oscillators/noise.
 * - Handles `visibilitychange` suspend/resume and degrades cleanly if Web Audio is unavailable.
 */

export const enum AudioCueCode {
  StartOpeningChime = 1,
  StartIllegalWarningDrone = 2,
  OpeningFlyUpWhoosh = 3,
  FadeOutBgm = 4,
  MenuCursorMove = 5,
  MenuConfirm = 6,
  MenuCancel = 7,
  StartMenuAmbience = 8,
  MenuValueStep = 9,
}

export interface GainParamLike {
  value: number;
  setValueAtTime(value: number, startTime: number): void;
  linearRampToValueAtTime(value: number, endTime: number): void;
  exponentialRampToValueAtTime(value: number, endTime: number): void;
}

export interface AudioNodeLike {
  connect(destination: unknown): unknown;
  disconnect(): void;
}

export interface GainNodeLike extends AudioNodeLike {
  gain: GainParamLike;
}

export interface OscillatorNodeLike extends AudioNodeLike {
  type: OscillatorType;
  frequency: GainParamLike;
  detune: GainParamLike;
  start(when?: number): void;
  stop(when?: number): void;
}

export interface BiquadFilterNodeLike extends AudioNodeLike {
  type: BiquadFilterType;
  frequency: GainParamLike;
  Q: GainParamLike;
}

export interface AudioContextLike {
  readonly state: AudioContextState;
  readonly currentTime: number;
  readonly destination: unknown;
  resume(): Promise<void>;
  suspend(): Promise<void>;
  close(): Promise<void>;
  createGain(): GainNodeLike;
  createOscillator(): OscillatorNodeLike;
  createBiquadFilter(): BiquadFilterNodeLike;
}

export interface AudioMixerSettings {
  muted: boolean;
  masterVolume: number;
  ambientVolume: number;
  sfxVolume: number;
  reducedMotion?: boolean;
}

export type AudioEngineStatus = "unactivated" | "running" | "suspended" | "muted" | "unavailable";

export interface AudioEngineOptions {
  contextFactory?: () => AudioContextLike | null;
}

function defaultContextFactory(): AudioContextLike | null {
  if (typeof window === "undefined") return null;
  const Ctor =
    window.AudioContext ??
    (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (typeof Ctor !== "function") return null;
  return new Ctor() as unknown as AudioContextLike;
}

export class AudioEngine {
  private readonly contextFactory: () => AudioContextLike | null;
  private ctx: AudioContextLike | null = null;
  private masterGain: GainNodeLike | null = null;
  private ambientGain: GainNodeLike | null = null;
  private sfxGain: GainNodeLike | null = null;
  private droneOscs: OscillatorNodeLike[] = [];

  private activated = false;
  private unavailable = false;
  private hiddenSuspended = false;
  private resumeCallCount = 0;

  private mixer: AudioMixerSettings = {
    muted: false,
    masterVolume: 0.75,
    ambientVolume: 0.7,
    sfxVolume: 0.8,
    reducedMotion: false,
  };

  private readonly onVisibilityChange = (): void => {
    if (typeof document === "undefined" || !this.ctx || !this.activated) return;
    if (document.hidden) {
      this.hiddenSuspended = true;
      void this.ctx.suspend().catch(() => undefined);
    } else if (this.hiddenSuspended) {
      this.hiddenSuspended = false;
      if (!this.mixer.muted) {
        void this.ctx.resume().catch(() => undefined);
      }
    }
  };

  constructor(options: AudioEngineOptions = {}) {
    this.contextFactory = options.contextFactory ?? defaultContextFactory;
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", this.onVisibilityChange);
    }
  }

  getStatus(): AudioEngineStatus {
    if (this.unavailable) return "unavailable";
    if (!this.activated || !this.ctx) return "unactivated";
    if (this.mixer.muted) return "muted";
    return this.ctx.state === "running" ? "running" : "suspended";
  }

  getContextState(): AudioContextState | "uninitialized" {
    return this.ctx ? this.ctx.state : "uninitialized";
  }

  getResumeCallCount(): number {
    return this.resumeCallCount;
  }

  isActivated(): boolean {
    return this.activated;
  }

  /**
   * MUST be called synchronously inside the user's single activation click/keydown handler
   * with ZERO preceding `await`s so browser user-activation unlocks `AudioContext.resume()`.
   */
  activateFromUserGesture(): Promise<boolean> {
    if (this.unavailable) {
      return Promise.resolve(false);
    }

    try {
      if (!this.ctx) {
        this.ctx = this.contextFactory();
        if (!this.ctx) {
          this.unavailable = true;
          return Promise.resolve(false);
        }
        this.setupBusAndAmbientPad(this.ctx);
      }

      // Synchronous call to `ctx.resume()` directly inside the gesture call stack!
      this.resumeCallCount++;
      const resumePromise = this.ctx.resume();
      this.activated = true;
      this.applyMixerGains();

      return resumePromise
        .then(() => true)
        .catch(() => {
          return false;
        });
    } catch {
      this.unavailable = true;
      return Promise.resolve(false);
    }
  }

  setMixer(next: Partial<AudioMixerSettings>): void {
    this.mixer = {
      ...this.mixer,
      ...next,
      masterVolume: Math.max(0, Math.min(1, next.masterVolume ?? this.mixer.masterVolume)),
      ambientVolume: Math.max(0, Math.min(1, next.ambientVolume ?? this.mixer.ambientVolume)),
      sfxVolume: Math.max(0, Math.min(1, next.sfxVolume ?? this.mixer.sfxVolume)),
    };
    this.applyMixerGains();
  }

  getMixer(): Readonly<AudioMixerSettings> {
    return this.mixer;
  }

  dispatchCues(cues: readonly number[]): void {
    if (!this.activated || !this.ctx || this.mixer.muted || !this.sfxGain || !this.ambientGain) {
      return;
    }
    for (const cue of cues) {
      this.triggerCue(cue as AudioCueCode);
    }
  }

  dispose(): void {
    if (typeof document !== "undefined") {
      document.removeEventListener("visibilitychange", this.onVisibilityChange);
    }
    for (const osc of this.droneOscs) {
      try {
        osc.stop();
        osc.disconnect();
      } catch {
        // Ignore already-stopped oscillator
      }
    }
    this.droneOscs = [];
    if (this.ctx) {
      void this.ctx.close().catch(() => undefined);
      this.ctx = null;
    }
    this.activated = false;
  }

  private setupBusAndAmbientPad(ctx: AudioContextLike): void {
    const master = ctx.createGain();
    const ambient = ctx.createGain();
    const sfx = ctx.createGain();
    const filter = ctx.createBiquadFilter();
    filter.type = "lowpass";
    filter.frequency.value = 620;
    filter.Q.value = 1.2;

    ambient.connect(filter);
    filter.connect(master);
    sfx.connect(master);
    master.connect(ctx.destination);

    this.masterGain = master;
    this.ambientGain = ambient;
    this.sfxGain = sfx;

    // Clean-room warm suspended atmospheric pad (D2, A2, E3, F#3 gentle detuned sines/triangles)
    const freqs = [73.42, 110.0, 164.81, 185.0];
    const now = ctx.currentTime;
    freqs.forEach((hz, idx) => {
      const osc = ctx.createOscillator();
      const voiceGain = ctx.createGain();
      osc.type = idx % 2 === 0 ? "sine" : "triangle";
      osc.frequency.setValueAtTime(hz, now);
      osc.detune.setValueAtTime((idx - 1.5) * 4.5, now);
      voiceGain.gain.setValueAtTime(0.06 / (idx * 0.4 + 1), now);
      osc.connect(voiceGain);
      voiceGain.connect(ambient);
      osc.start(now);
      this.droneOscs.push(osc);
    });
  }

  private applyMixerGains(): void {
    if (!this.ctx || !this.masterGain || !this.ambientGain || !this.sfxGain) return;
    const now = this.ctx.currentTime;
    const masterTarget = this.mixer.muted ? 0.0 : this.mixer.masterVolume;
    const ambientScale = this.mixer.reducedMotion ? 0.65 : 1.0;
    this.masterGain.gain.setValueAtTime(masterTarget, now);
    this.ambientGain.gain.setValueAtTime(this.mixer.ambientVolume * ambientScale, now);
    this.sfxGain.gain.setValueAtTime(this.mixer.sfxVolume, now);
  }

  private playSynthTone(
    freqStart: number,
    freqEnd: number,
    durationSec: number,
    peakGain: number,
    wave: OscillatorType = "sine",
  ): void {
    if (!this.ctx || !this.sfxGain) return;
    const now = this.ctx.currentTime;
    const osc = this.ctx.createOscillator();
    const env = this.ctx.createGain();
    osc.type = wave;
    osc.frequency.setValueAtTime(freqStart, now);
    if (freqEnd !== freqStart) {
      osc.frequency.exponentialRampToValueAtTime(Math.max(20, freqEnd), now + durationSec);
    }
    env.gain.setValueAtTime(0.0001, now);
    env.gain.linearRampToValueAtTime(peakGain, now + Math.min(0.018, durationSec * 0.25));
    env.gain.exponentialRampToValueAtTime(0.0001, now + durationSec);

    osc.connect(env);
    env.connect(this.sfxGain);
    osc.start(now);
    osc.stop(now + durationSec + 0.02);
  }

  private triggerCue(cue: AudioCueCode): void {
    switch (cue) {
      case AudioCueCode.StartOpeningChime:
        // Original clean-room crystalline fifths arpeggio
        this.playSynthTone(293.66, 293.66, 1.6, 0.16, "sine");
        this.playSynthTone(440.0, 440.0, 1.9, 0.12, "triangle");
        this.playSynthTone(659.25, 659.25, 2.2, 0.09, "sine");
        break;
      case AudioCueCode.StartIllegalWarningDrone:
        // Low minor-second warning pulse
        this.playSynthTone(98.0, 92.5, 1.8, 0.22, "sawtooth");
        this.playSynthTone(103.83, 98.0, 1.8, 0.14, "triangle");
        break;
      case AudioCueCode.OpeningFlyUpWhoosh:
        this.playSynthTone(140.0, 520.0, 0.85, 0.14, "sine");
        break;
      case AudioCueCode.FadeOutBgm:
        if (this.ctx && this.ambientGain) {
          this.ambientGain.gain.linearRampToValueAtTime(
            this.mixer.ambientVolume * 0.25,
            this.ctx.currentTime + 0.6,
          );
        }
        break;
      case AudioCueCode.StartMenuAmbience:
        if (this.ctx && this.ambientGain) {
          this.ambientGain.gain.linearRampToValueAtTime(
            this.mixer.ambientVolume,
            this.ctx.currentTime + 0.8,
          );
        }
        break;
      case AudioCueCode.MenuCursorMove:
        this.playSynthTone(740.0, 880.0, 0.065, 0.1, "sine");
        break;
      case AudioCueCode.MenuConfirm:
        this.playSynthTone(587.33, 1174.66, 0.16, 0.14, "triangle");
        break;
      case AudioCueCode.MenuCancel:
        this.playSynthTone(523.25, 329.63, 0.14, 0.12, "sine");
        break;
      case AudioCueCode.MenuValueStep:
        this.playSynthTone(987.77, 1046.5, 0.045, 0.08, "sine");
        break;
    }
  }
}
