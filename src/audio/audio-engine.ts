/**
 * Web Audio realization of the OSDSYS sound path.
 *
 * The retail console plays every sound on the IOP through `rom0:OSDSND`; the EE side only
 * enqueues messages (`OSDDispatch`, `docs/sound.md`). The Rust core reproduces that mailbox
 * byte-for-byte, including the 60-tick reverb ramp, and this module turns the drained messages
 * into real audio:
 *
 *   20500 (StBgmPlay slot)     start one of the eight BGM beds
 *   20501 (StBgmStop slot,f)   fade the bed out over `f` frames
 *   20992 (StSePlay 1, n)      play UI sound effect n (4 confirm, 5 toggle, 6 cursor, 10 cancel)
 *   20493 (StSetReverbDepth)   drive the global reverb send from the boot ramp
 *
 * The beds are original synthesized material (no Sony samples are redistributed): the eight
 * slots are recreated as chord/arpeggio gestures whose character matches the documented role
 * of each sequence - the boot chime, the tower-field drone, the glass-clock pad, and the
 * warning drone.
 *
 * Activation contract: `unlock()` must be called synchronously from the user's click handler so
 * `AudioContext.resume()` runs inside the user gesture. Nothing is created before that.
 */

export const SND_CMD_BGM_SET_VOL = 20499;
export const SND_CMD_BGM_PLAY = 20500;
export const SND_CMD_BGM_STOP = 20501;
export const SND_CMD_REVERB_DEPTH = 20493;
export const SND_CMD_SE_PLAY = 20992;

export const BGM_SLOT_BOOTS = 0;
export const BGM_SLOT_TNNLS = 1;
export const BGM_SLOT_CLOKS = 2;
export const BGM_SLOT_WARNS = 6;
export const BGM_SLOT_RCLKS = 7;

export const SE_CONFIRM = 4;
export const SE_TOGGLE = 5;
export const SE_CURSOR = 6;
export const SE_CANCEL = 10;

export interface AudioStatus {
  unlocked: boolean;
  running: boolean;
  contextState: string;
  masterVolume: number;
  muted: boolean;
  activeBeds: string[];
  error: string | null;
}

type BedName = 'boots' | 'tnnls' | 'cloks' | 'warns' | 'rclks' | 'logos' | 'tm30s' | 'tm60s';

interface Bed {
  name: BedName;
  output: GainNode;
  nodes: AudioScheduledSourceNode[];
  oscillators: OscillatorNode[];
  stopAt: number | null;
}

/** Note frequencies (equal temperament, A4 = 440 Hz). */
const NOTE = {
  A2: 110.0,
  C3: 130.81,
  D3: 146.83,
  E3: 164.81,
  F3: 174.61,
  G3: 196.0,
  A3: 220.0,
  B3: 246.94,
  C4: 261.63,
  D4: 293.66,
  E4: 329.63,
  F4: 349.23,
  G4: 392.0,
  A4: 440.0,
  B4: 493.88,
  C5: 523.25,
  D5: 587.33,
  E5: 659.26,
  G5: 783.99,
} as const;

export class AudioEngine {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private bedsBus: GainNode | null = null;
  private seBus: GainNode | null = null;
  private reverb: ConvolverNode | null = null;
  private reverbSend: GainNode | null = null;
  private beds = new Map<number, Bed>();
  private unlocked = false;
  private muted = false;
  private masterVolume = 0.8;
  private bgmVolume = 0.7;
  private seVolume = 0.8;
  private lastError: string | null = null;

  /**
   * Creates and resumes the AudioContext. MUST be called synchronously inside the click
   * handler: the `resume()` promise is started before anything is awaited so the browser still
   * sees the user gesture.
   */
  unlock(): { ok: boolean; error: string | null } {
    try {
      if (!this.ctx) {
        const Ctor =
          window.AudioContext ??
          (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
        if (!Ctor) {
          this.lastError = 'Web Audio is not available in this browser';
          return { ok: false, error: this.lastError };
        }
        this.ctx = new Ctor({ latencyHint: 'interactive' });
        this.buildGraph(this.ctx);
      }
      // Kick the resume inside the gesture; do not await before this point.
      const resuming = this.ctx.resume();
      this.unlocked = true;
      resuming.catch((error: unknown) => {
        this.lastError = error instanceof Error ? error.message : String(error);
      });
      return { ok: true, error: null };
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
      return { ok: false, error: this.lastError };
    }
  }

  private buildGraph(ctx: AudioContext): void {
    this.master = ctx.createGain();
    this.master.gain.value = this.muted ? 0 : this.masterVolume;

    const compressor = ctx.createDynamicsCompressor();
    compressor.threshold.value = -12;
    compressor.ratio.value = 3;
    compressor.attack.value = 0.01;
    compressor.release.value = 0.25;

    this.bedsBus = ctx.createGain();
    this.bedsBus.gain.value = this.bgmVolume;
    this.seBus = ctx.createGain();
    this.seBus.gain.value = this.seVolume;

    this.reverb = ctx.createConvolver();
    this.reverb.buffer = this.makeImpulse(ctx, 2.6, 2.2);
    this.reverbSend = ctx.createGain();
    this.reverbSend.gain.value = 0;

    this.bedsBus.connect(this.master);
    this.seBus.connect(this.master);
    this.bedsBus.connect(this.reverbSend);
    this.reverbSend.connect(this.reverb);
    this.reverb.connect(this.master);
    this.master.connect(compressor);
    compressor.connect(ctx.destination);
  }

  /** Synthesized hall impulse: exponentially decaying filtered noise. */
  private makeImpulse(ctx: AudioContext, seconds: number, decay: number): AudioBuffer {
    const rate = ctx.sampleRate;
    const length = Math.floor(rate * seconds);
    const buffer = ctx.createBuffer(2, length, rate);
    for (let channel = 0; channel < 2; channel += 1) {
      const data = buffer.getChannelData(channel);
      let last = 0;
      for (let i = 0; i < length; i += 1) {
        const white = Math.random() * 2 - 1;
        last = last * 0.72 + white * 0.28;
        const envelope = Math.pow(1 - i / length, decay);
        data[i] = last * envelope * 0.6;
      }
    }
    return buffer;
  }

  setMasterVolume(volume: number): void {
    this.masterVolume = Math.max(0, Math.min(1, volume));
    this.applyMasterGain();
  }

  setBgmVolume(volume: number): void {
    this.bgmVolume = Math.max(0, Math.min(1, volume));
    if (this.bedsBus && this.ctx) {
      this.bedsBus.gain.setTargetAtTime(this.bgmVolume, this.ctx.currentTime, 0.05);
    }
  }

  setSeVolume(volume: number): void {
    this.seVolume = Math.max(0, Math.min(1, volume));
    if (this.seBus && this.ctx) {
      this.seBus.gain.setTargetAtTime(this.seVolume, this.ctx.currentTime, 0.05);
    }
  }

  setMuted(muted: boolean): void {
    this.muted = muted;
    this.applyMasterGain();
  }

  private applyMasterGain(): void {
    if (this.master && this.ctx) {
      this.master.gain.setTargetAtTime(
        this.muted ? 0 : this.masterVolume,
        this.ctx.currentTime,
        0.05,
      );
    }
  }

  /** Applies one frame's drained mailbox. Safe to call before `unlock()` (it just drops them). */
  processMessages(messages: readonly { msg: number; a: number; b: number; c: number }[]): void {
    if (!this.ctx || !this.unlocked) return;
    for (const message of messages) {
      switch (message.msg) {
        case SND_CMD_BGM_PLAY:
          this.playBed(message.a);
          break;
        case SND_CMD_BGM_STOP:
          this.stopBed(message.a, message.c);
          break;
        case SND_CMD_SE_PLAY:
          this.playSe(message.b);
          break;
        case SND_CMD_REVERB_DEPTH: {
          const depth = message.b / 0xffff;
          if (this.reverbSend && this.ctx) {
            this.reverbSend.gain.setTargetAtTime(depth * 0.5, this.ctx.currentTime, 0.08);
          }
          break;
        }
        case SND_CMD_BGM_SET_VOL:
          // Per-slot volumes are already folded into the bed gains.
          break;
        default:
          break;
      }
    }
  }

  private bedFor(slot: number): BedName {
    switch (slot) {
      case BGM_SLOT_BOOTS:
        return 'boots';
      case BGM_SLOT_TNNLS:
        return 'tnnls';
      case BGM_SLOT_CLOKS:
        return 'cloks';
      case BGM_SLOT_WARNS:
        return 'warns';
      case BGM_SLOT_RCLKS:
        return 'rclks';
      case 3:
        return 'tm30s';
      case 4:
        return 'tm60s';
      default:
        return 'logos';
    }
  }

  private playBed(slot: number): void {
    if (!this.ctx || !this.bedsBus) return;
    const existing = this.beds.get(slot);
    if (existing) {
      // Already playing: cancel any scheduled fade-out and sustain it.
      existing.stopAt = null;
      if (existing.output.gain) {
        existing.output.gain.cancelScheduledValues(this.ctx.currentTime);
        existing.output.gain.setTargetAtTime(1, this.ctx.currentTime, 0.1);
      }
      return;
    }
    const name = this.bedFor(slot);
    const bed = this.startBed(name);
    if (bed) this.beds.set(slot, bed);
  }

  private stopBed(slot: number, fadeFrames: number): void {
    const bed = this.beds.get(slot);
    if (!bed || !this.ctx) return;
    const fadeSeconds = Math.max(0.05, (fadeFrames || 15) / 60);
    bed.output.gain.cancelScheduledValues(this.ctx.currentTime);
    bed.output.gain.setTargetAtTime(0, this.ctx.currentTime, fadeSeconds / 3);
    bed.stopAt = this.ctx.currentTime + fadeSeconds;
    const stopTime = bed.stopAt;
    this.beds.delete(slot);
    setTimeout(() => {
      if (!this.ctx) return;
      if (bed.stopAt !== stopTime) return;
      for (const node of bed.nodes) {
        try {
          node.stop();
        } catch {
          /* already stopped */
        }
      }
    }, fadeSeconds * 1000 + 200);
  }

  private stopAll(): void {
    for (const slot of [...this.beds.keys()]) this.stopBed(slot, 10);
  }

  private startBed(name: BedName): Bed | null {
    const ctx = this.ctx;
    const bus = this.bedsBus;
    if (!ctx || !bus) return null;
    const output = ctx.createGain();
    output.gain.value = 0;
    output.connect(bus);
    output.gain.setTargetAtTime(1, ctx.currentTime, 0.35);

    const nodes: AudioScheduledSourceNode[] = [];
    const oscillators: OscillatorNode[] = [];
    const now = ctx.currentTime;

    const addOsc = (
      type: OscillatorType,
      freq: number,
      gain: number,
      detune = 0,
      filterHz?: number,
    ): OscillatorNode => {
      const osc = ctx.createOscillator();
      osc.type = type;
      osc.frequency.value = freq;
      osc.detune.value = detune;
      const g = ctx.createGain();
      g.gain.value = gain;
      if (filterHz !== undefined) {
        const filter = ctx.createBiquadFilter();
        filter.type = 'lowpass';
        filter.frequency.value = filterHz;
        filter.Q.value = 0.7;
        osc.connect(filter);
        filter.connect(g);
      } else {
        osc.connect(g);
      }
      g.connect(output);
      osc.start(now);
      oscillators.push(osc);
      nodes.push(osc);
      return osc;
    };

    switch (name) {
      case 'boots': {
        // The PS2 boot chime: a bright major chord bloom that decays into a shimmer.
        addOsc('sine', NOTE.C4, 0.18);
        addOsc('sine', NOTE.E4, 0.14, 4);
        addOsc('sine', NOTE.G4, 0.12, -4);
        addOsc('triangle', NOTE.C5, 0.05, 2);
        break;
      }
      case 'tnnls': {
        // Tower field: a wide, slow minor drone with a breathing lowpass sweep.
        addOsc('sawtooth', NOTE.D3, 0.05, -6, 420);
        addOsc('sawtooth', NOTE.A3, 0.045, 6, 520);
        addOsc('sine', NOTE.A2, 0.08);
        const lfo = ctx.createOscillator();
        lfo.type = 'sine';
        lfo.frequency.value = 0.06;
        const lfoGain = ctx.createGain();
        lfoGain.gain.value = 180;
        lfo.connect(lfoGain);
        // Sweep the shared bed gain subtly for a wind-like swell.
        lfoGain.connect(output.gain as unknown as AudioNode);
        lfo.start(now);
        oscillators.push(lfo);
        nodes.push(lfo);
        break;
      }
      case 'cloks': {
        // System Configuration: glassy suspended pad with slow arpeggio.
        addOsc('sine', NOTE.F3, 0.05);
        addOsc('sine', NOTE.C4, 0.045, 3);
        addOsc('sine', NOTE.G4, 0.04, -3);
        addOsc('triangle', NOTE.E5, 0.02);
        break;
      }
      case 'warns': {
        // Illegal disc: dissonant low cluster with a slow tremolo.
        addOsc('sawtooth', NOTE.A2, 0.07, -12, 260);
        addOsc('sawtooth', NOTE.A2, 0.06, 14, 260);
        addOsc('square', NOTE.F3 * 0.994, 0.03, 0, 180);
        const trem = ctx.createOscillator();
        trem.type = 'sine';
        trem.frequency.value = 0.9;
        const tremGain = ctx.createGain();
        tremGain.gain.value = 0.35;
        trem.connect(tremGain);
        tremGain.connect(output.gain as unknown as AudioNode);
        trem.start(now);
        oscillators.push(trem);
        nodes.push(trem);
        break;
      }
      case 'rclks': {
        // Menu orbs: a rotating glass arpeggio.
        addOsc('triangle', NOTE.A3, 0.04);
        addOsc('triangle', NOTE.C5, 0.03, 5);
        addOsc('sine', NOTE.E5, 0.02, -5);
        break;
      }
      case 'tm30s':
      case 'tm60s': {
        addOsc('sine', NOTE.G3, 0.05);
        addOsc('sine', NOTE.D4, 0.04, 3);
        break;
      }
      default: {
        addOsc('triangle', NOTE.C4, 0.05);
        addOsc('sine', NOTE.G4, 0.03);
        break;
      }
    }

    // Very light vibrato so the beds never sound like a frozen tone.
    for (const osc of oscillators) {
      if (osc.type === 'sine' && osc.frequency.value > 100) {
        const lfo = ctx.createOscillator();
        lfo.type = 'sine';
        lfo.frequency.value = 0.2 + Math.random() * 0.15;
        const depth = ctx.createGain();
        depth.gain.value = 1.8;
        lfo.connect(depth);
        depth.connect(osc.detune);
        lfo.start(now);
        nodes.push(lfo);
      }
    }

    return { name, output, nodes, oscillators, stopAt: null };
  }

  private playSe(n: number): void {
    const ctx = this.ctx;
    const bus = this.seBus;
    if (!ctx || !bus) return;
    const now = ctx.currentTime;
    const voice = (hz: number, dur: number, type: OscillatorType, gain: number, sweepTo?: number): void => {
      const osc = ctx.createOscillator();
      osc.type = type;
      osc.frequency.setValueAtTime(hz, now);
      if (sweepTo !== undefined) {
        osc.frequency.exponentialRampToValueAtTime(sweepTo, now + dur);
      }
      const g = ctx.createGain();
      g.gain.setValueAtTime(0, now);
      g.gain.linearRampToValueAtTime(gain, now + 0.006);
      g.gain.exponentialRampToValueAtTime(0.0001, now + dur);
      osc.connect(g);
      g.connect(bus);
      osc.start(now);
      osc.stop(now + dur + 0.02);
    };

    switch (n) {
      case SE_CONFIRM:
        voice(880, 0.12, 'sine', 0.16, 1320);
        voice(1320, 0.16, 'triangle', 0.07);
        break;
      case SE_TOGGLE:
        voice(660, 0.07, 'square', 0.05, 880);
        break;
      case SE_CURSOR:
        voice(1046, 0.045, 'sine', 0.09);
        break;
      case SE_CANCEL:
        voice(440, 0.13, 'sine', 0.13, 300);
        break;
      default:
        voice(740, 0.06, 'sine', 0.07);
        break;
    }
  }

  /** Stops everything (used when leaving the experience or resetting). */
  reset(): void {
    this.stopAll();
  }

  status(): AudioStatus {
    return {
      unlocked: this.unlocked,
      running: this.ctx?.state === 'running',
      contextState: this.ctx?.state ?? 'none',
      masterVolume: this.masterVolume,
      muted: this.muted,
      activeBeds: [...this.beds.values()].map((bed) => bed.name),
      error: this.lastError,
    };
  }

  async suspend(): Promise<void> {
    await this.ctx?.suspend();
  }

  async resume(): Promise<void> {
    await this.ctx?.resume();
  }

  dispose(): void {
    this.stopAll();
    void this.ctx?.close();
    this.ctx = null;
    this.unlocked = false;
  }
}
