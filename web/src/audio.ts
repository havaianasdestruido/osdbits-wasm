/**
 * Procedural audio.
 *
 * The original plays sequences from the BIOS (`sound.c` drives the SPU2). None
 * of that audio may ship, so the browser build synthesises its own: a low
 * drone for the opening, a filtered noise sweep during the fly-up, a bell on
 * scene end and a sparse pad in the illegal scene. Everything is created lazily
 * from a user gesture (browsers refuse to start an `AudioContext` otherwise).
 *
 * This is clearly *not* the original soundtrack — see docs/LIMITATIONS.md.
 */

export interface AudioCapabilities {
  available: boolean;
  reason: string;
}

export class AudioEngine {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private drone: { osc: OscillatorNode[]; gain: GainNode } | null = null;
  private noise: { source: AudioBufferSourceNode; gain: GainNode; filter: BiquadFilterNode } | null =
    null;
  private volume = 0.6;
  private muted = true;

  /** Feature probe; safe to call before any gesture. */
  static capabilities(): AudioCapabilities {
    const w = globalThis as { AudioContext?: unknown; webkitAudioContext?: unknown };
    const Ctor = w.AudioContext ?? w.webkitAudioContext;
    if (typeof Ctor !== "function") {
      return { available: false, reason: "Web Audio is not available in this browser" };
    }
    return { available: true, reason: "" };
  }

  get started(): boolean {
    return this.ctx !== null;
  }

  /**
   * Create/resume the context. MUST be called synchronously inside a user
   * gesture handler — the whole point of the single "Start OSDSYS" click.
   */
  async start(volume: number): Promise<void> {
    this.volume = Math.min(1, Math.max(0, volume));
    if (this.ctx === null) {
      const w = globalThis as unknown as { AudioContext: typeof AudioContext };
      this.ctx = new w.AudioContext({ latencyHint: "interactive" });
      this.master = this.ctx.createGain();
      this.master.gain.value = this.muted ? 0 : this.volume;
      this.master.connect(this.ctx.destination);
    }
    if (this.ctx.state === "suspended") {
      await this.ctx.resume();
    }
    this.muted = false;
    this.applyGain();
  }

  private applyGain(): void {
    if (!this.master || !this.ctx) return;
    const target = this.muted ? 0 : this.volume;
    this.master.gain.cancelScheduledValues(this.ctx.currentTime);
    this.master.gain.linearRampToValueAtTime(target, this.ctx.currentTime + 0.08);
  }

  setVolume(volume: number): void {
    this.volume = Math.min(1, Math.max(0, volume));
    this.applyGain();
  }

  setMuted(muted: boolean): void {
    this.muted = muted;
    this.applyGain();
  }

  /** Opening drone: two detuned sines plus a low-passed fifth. */
  startDrone(): void {
    const ctx = this.ctx;
    if (!ctx || !this.master || this.drone) return;
    const gain = ctx.createGain();
    gain.gain.value = 0;
    const filter = ctx.createBiquadFilter();
    filter.type = "lowpass";
    filter.frequency.value = 520;
    filter.Q.value = 0.7;
    const osc: OscillatorNode[] = [];
    for (const [freq, detune, level] of [
      [55, 0, 0.5],
      [82.5, 4, 0.22],
      [110, -6, 0.12],
    ] as const) {
      const o = ctx.createOscillator();
      o.type = "sine";
      o.frequency.value = freq;
      o.detune.value = detune;
      const g = ctx.createGain();
      g.gain.value = level;
      o.connect(g).connect(filter);
      o.start();
      osc.push(o);
    }
    filter.connect(gain).connect(this.master);
    gain.gain.linearRampToValueAtTime(0.22, ctx.currentTime + 3);
    this.drone = { osc, gain };
  }

  /** Fly-up sweep: band-passed noise whose centre frequency follows `t`. */
  startWhoosh(): void {
    const ctx = this.ctx;
    if (!ctx || !this.master || this.noise) return;
    const length = Math.floor(ctx.sampleRate * 2);
    const buffer = ctx.createBuffer(1, length, ctx.sampleRate);
    const channel = buffer.getChannelData(0);
    let last = 0;
    for (let i = 0; i < length; i++) {
      // Brown-ish noise: integrated white noise, bounded.
      last = (last + (Math.random() * 2 - 1) * 0.02) * 0.995;
      channel[i] = last * 3;
    }
    const source = ctx.createBufferSource();
    source.buffer = buffer;
    source.loop = true;
    const filter = ctx.createBiquadFilter();
    filter.type = "bandpass";
    filter.frequency.value = 180;
    filter.Q.value = 1.2;
    const gain = ctx.createGain();
    gain.gain.value = 0;
    source.connect(filter).connect(gain).connect(this.master);
    source.start();
    this.noise = { source, gain, filter };
  }

  /** Drive the whoosh with the camera height (72 -> 320 is the fly-up). */
  updateFlyUp(z: number): void {
    if (!this.ctx || !this.noise) return;
    const t = Math.min(1, Math.max(0, (z - 60) / 280));
    const now = this.ctx.currentTime;
    this.noise.filter.frequency.setTargetAtTime(180 + t * 1400, now, 0.12);
    this.noise.gain.gain.setTargetAtTime(0.05 + t * 0.25, now, 0.15);
  }

  /** One bell on the scene end / menu confirm. */
  chime(frequency = 440): void {
    const ctx = this.ctx;
    if (!ctx || !this.master) return;
    const now = ctx.currentTime;
    for (const [ratio, level, decay] of [
      [1, 0.18, 1.6],
      [2.01, 0.08, 1.1],
      [3.02, 0.04, 0.8],
    ] as const) {
      const o = ctx.createOscillator();
      o.type = "sine";
      o.frequency.value = frequency * ratio;
      const g = ctx.createGain();
      g.gain.setValueAtTime(0, now);
      g.gain.linearRampToValueAtTime(level, now + 0.01);
      g.gain.exponentialRampToValueAtTime(0.0001, now + decay);
      o.connect(g).connect(this.master);
      o.start(now);
      o.stop(now + decay + 0.05);
    }
  }

  /** Map a core camera event onto a sound, as `OSDDispatch` would. */
  handleEvent(kind: number, arg: number): void {
    if (kind === 1) {
      this.startDrone();
      this.startWhoosh();
      return;
    }
    if (kind === 2) {
      this.fadeOut(Math.max(0.2, arg / 30));
      return;
    }
    if (kind === 3) {
      this.chime(392);
    }
  }

  fadeOut(seconds: number): void {
    const ctx = this.ctx;
    if (!ctx || !this.master) return;
    const now = ctx.currentTime;
    this.master.gain.cancelScheduledValues(now);
    this.master.gain.setValueAtTime(this.master.gain.value, now);
    this.master.gain.linearRampToValueAtTime(0.0001, now + seconds);
  }

  /** Stop everything (scene restart) without tearing down the context. */
  silence(): void {
    const ctx = this.ctx;
    if (!ctx) return;
    for (const osc of this.drone?.osc ?? []) {
      try {
        osc.stop();
      } catch {
        /* already stopped */
      }
    }
    try {
      this.noise?.source.stop();
    } catch {
      /* already stopped */
    }
    this.drone = null;
    this.noise = null;
    if (this.master) {
      this.master.gain.cancelScheduledValues(ctx.currentTime);
      this.master.gain.setValueAtTime(this.volume, ctx.currentTime);
    }
  }

  async suspend(): Promise<void> {
    if (this.ctx && this.ctx.state === "running") await this.ctx.suspend();
  }

  async resume(): Promise<void> {
    if (this.ctx && this.ctx.state === "suspended") await this.ctx.resume();
  }

  get context(): AudioContext | null {
    return this.ctx;
  }

  get state(): string {
    return this.ctx?.state ?? "none";
  }
}
