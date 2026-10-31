import { describe, expect, it } from "vitest";
import {
  AudioCueCode,
  AudioEngine,
  type AudioContextLike,
  type BiquadFilterNodeLike,
  type GainNodeLike,
  type GainParamLike,
  type OscillatorNodeLike,
} from "../../web/src/audio/audioEngine";

function makeMockParam(initial = 0): GainParamLike {
  return {
    value: initial,
    setValueAtTime(v: number) {
      this.value = v;
    },
    linearRampToValueAtTime(v: number) {
      this.value = v;
    },
    exponentialRampToValueAtTime(v: number) {
      this.value = v;
    },
  };
}

function createMockAudioContext(callLog: string[]): AudioContextLike {
  let state: AudioContextState = "suspended";
  return {
    get state() {
      return state;
    },
    currentTime: 0,
    destination: {},
    resume() {
      callLog.push("sync:AudioContext.resume");
      state = "running";
      return Promise.resolve();
    },
    suspend() {
      callLog.push("AudioContext.suspend");
      state = "suspended";
      return Promise.resolve();
    },
    close() {
      callLog.push("AudioContext.close");
      state = "closed";
      return Promise.resolve();
    },
    createGain(): GainNodeLike {
      return {
        gain: makeMockParam(1),
        connect() {
          return undefined;
        },
        disconnect() {},
      };
    },
    createOscillator(): OscillatorNodeLike {
      return {
        type: "sine",
        frequency: makeMockParam(440),
        detune: makeMockParam(0),
        connect() {
          return undefined;
        },
        disconnect() {},
        start() {
          callLog.push("osc.start");
        },
        stop() {
          callLog.push("osc.stop");
        },
      };
    },
    createBiquadFilter(): BiquadFilterNodeLike {
      return {
        type: "lowpass",
        frequency: makeMockParam(600),
        Q: makeMockParam(1),
        connect() {
          return undefined;
        },
        disconnect() {},
      };
    },
  };
}

describe("Phase 8 — Browser-Safe Clean-Room AudioEngine", () => {
  it("stays completely silent prior to gesture and invokes AudioContext.resume() synchronously inside gesture handler", async () => {
    const callLog: string[] = [];
    const engine = new AudioEngine({
      contextFactory: () => createMockAudioContext(callLog),
    });

    // Pre-activation: zero nodes created, zero resume calls, cue dispatch is a safe no-op
    expect(engine.getStatus()).toBe("unactivated");
    expect(engine.getContextState()).toBe("uninitialized");
    engine.dispatchCues([AudioCueCode.MenuConfirm]);
    expect(callLog).toEqual([]);

    // Simulate user's single click handler: verify `resume()` executes synchronously BEFORE any microtask!
    let resumeRanSynchronouslyInHandler = false;
    const clickHandler = (): Promise<boolean> => {
      const p = engine.activateFromUserGesture();
      resumeRanSynchronouslyInHandler = callLog.includes("sync:AudioContext.resume");
      return p;
    };

    const ok = await clickHandler();
    expect(resumeRanSynchronouslyInHandler).toBe(true);
    expect(ok).toBe(true);
    expect(engine.getStatus()).toBe("running");
    expect(engine.getResumeCallCount()).toBe(1);

    // Dispatch cues while running -> triggers foley oscillator
    const beforeLen = callLog.length;
    engine.dispatchCues([AudioCueCode.MenuCursorMove, AudioCueCode.MenuConfirm]);
    expect(callLog.length).toBeGreaterThan(beforeLen);

    // Mute suppresses status & cue synthesis cleanly
    engine.setMixer({ muted: true });
    expect(engine.getStatus()).toBe("muted");
    const mutedLen = callLog.length;
    engine.dispatchCues([AudioCueCode.MenuCancel]);
    expect(callLog.length).toBe(mutedLen);

    engine.dispose();
  });

  it("degrades gracefully when AudioContext is unavailable in the environment", async () => {
    const engine = new AudioEngine({ contextFactory: () => null });
    const ok = await engine.activateFromUserGesture();
    expect(ok).toBe(false);
    expect(engine.getStatus()).toBe("unavailable");
    expect(() => engine.dispatchCues([AudioCueCode.StartOpeningChime])).not.toThrow();
    engine.dispose();
  });

  it("suspends on document visibilitychange (hidden) and resumes when visible again unless muted", async () => {
    const callLog: string[] = [];
    const engine = new AudioEngine({
      contextFactory: () => createMockAudioContext(callLog),
    });

    await engine.activateFromUserGesture();
    expect(engine.getStatus()).toBe("running");

    Object.defineProperty(document, "hidden", { configurable: true, get: () => true });
    document.dispatchEvent(new Event("visibilitychange"));
    expect(callLog).toContain("AudioContext.suspend");
    expect(engine.getStatus()).toBe("suspended");

    Object.defineProperty(document, "hidden", { configurable: true, get: () => false });
    document.dispatchEvent(new Event("visibilitychange"));
    expect(engine.getStatus()).toBe("running");

    // If muted while hidden, returning visible keeps audio muted
    engine.setMixer({ muted: true });
    Object.defineProperty(document, "hidden", { configurable: true, get: () => true });
    document.dispatchEvent(new Event("visibilitychange"));
    const resumesBefore = engine.getResumeCallCount();
    Object.defineProperty(document, "hidden", { configurable: true, get: () => false });
    document.dispatchEvent(new Event("visibilitychange"));
    expect(engine.getResumeCallCount()).toBe(resumesBefore);
    expect(engine.getStatus()).toBe("muted");

    engine.dispose();
  });
});
