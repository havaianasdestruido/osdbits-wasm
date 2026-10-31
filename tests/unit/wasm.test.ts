/**
 * WASM artifact + ABI tests. These instantiate the real compiled module in Node so a passing
 * native `cargo test` is never mistaken for proof that the browser artifact works.
 */

import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { beforeAll, describe, expect, it } from 'vitest';
import {
  GsBlendMode,
  Pad,
  RenderCmdKind,
  SceneMode,
  TextureId,
  WasmCore,
  decodeAudio,
  decodeCommands,
} from '../../src/wasm/bridge';

const WASM_PATH = resolve(process.cwd(), 'public/wasm/osdbits_core.wasm');

async function loadCore(): Promise<WasmCore> {
  const bytes = readFileSync(WASM_PATH);
  const module = await WebAssembly.compile(bytes);
  // Reuse the production loader by handing it a fetch-compatible response.
  return WasmCore.load(`data:application/wasm;base64,${bytes.toString('base64')}`);
}

describe('WASM artifact', () => {
  it('is built at the documented path and is a valid wasm module', async () => {
    expect(existsSync(WASM_PATH)).toBe(true);
    const bytes = readFileSync(WASM_PATH);
    expect(bytes.byteLength).toBeGreaterThan(20_000);
    expect(WebAssembly.validate(bytes)).toBe(true);
  });

  let core: WasmCore;
  beforeAll(async () => {
    core = await loadCore();
  });

  it('initialises and exposes the full ABI', () => {
    const formatVersion = core.init(1234, false);
    expect(formatVersion).toBeGreaterThanOrEqual(1);
    expect(core.textureCount()).toBe(25);
    expect(core.getMode()).toBe(SceneMode.Opening);
  });

  it('reports every static texture with its documented dimensions', () => {
    const expected: Array<[TextureId, number, number]> = [
      [TextureId.WhiteDummy, 8, 8],
      [TextureId.TexoSce, 256, 64],
      [TextureId.TexoFog0, 128, 64],
      [TextureId.TexoWal0, 128, 128],
      [TextureId.TexoPngE, 512, 64],
      [TextureId.TexcKabe, 128, 128],
      [TextureId.TexcBump, 64, 64],
      [TextureId.TexcRefa, 64, 64],
      [TextureId.TexcMaru, 64, 64],
      [TextureId.FntAscii, 256, 480],
      [TextureId.FntExOsd, 512, 80],
    ];
    for (const [id, w, h] of expected) {
      const tex = core.texture(id);
      expect(tex.width, `texture ${id} width`).toBe(w);
      expect(tex.height, `texture ${id} height`).toBe(h);
      expect(tex.rgba.length).toBe(w * h * 4);
    }
  });

  it('produces a non-empty, well-formed render command stream for every mode', () => {
    for (const mode of [SceneMode.Opening, SceneMode.Idle, SceneMode.Illegal, SceneMode.Menu]) {
      core.setMode(mode);
      for (let i = 0; i < 30; i += 1) core.step(0);
      const commands = decodeCommands(core.commandView());
      expect(commands.length, `mode ${mode}`).toBeGreaterThan(3);
      expect(commands[0]!.kind).toBe(RenderCmdKind.ClearTarget);
      const draws = commands.filter((c) => c.kind === RenderCmdKind.DrawTriangles);
      expect(draws.length, `mode ${mode} draws`).toBeGreaterThan(0);
      const vertices = core.vertexCount();
      expect(vertices).toBeGreaterThan(0);
      for (const draw of draws) {
        expect(draw.vertexCount % 3).toBe(0);
        expect(draw.firstVertex + draw.vertexCount).toBeLessThanOrEqual(vertices);
        expect(Object.values(GsBlendMode)).toContain(draw.blendMode);
      }
    }
  });

  it('advances the opening state machine into the menu and emits audio messages', () => {
    core.setMode(SceneMode.Opening);
    const audio: number[] = [];
    let sawMenu = false;
    for (let i = 0; i < 420; i += 1) {
      core.step(0);
      for (const event of decodeAudio(core.audioView())) audio.push(event.msg);
      if (core.getMode() === SceneMode.Menu) {
        sawMenu = true;
        break;
      }
    }
    expect(sawMenu).toBe(true);
    expect(audio).toContain(20500); // StBgmPlay
    expect(audio).toContain(20493); // StSetReverbDepth ramp
  });

  it('drives the menu UI with the region-swapped pad word', () => {
    core.init(1, false); // World region: physical Cross confirms
    core.setMode(SceneMode.Menu);
    for (let i = 0; i < 40; i += 1) core.step(0);
    expect(core.state().screen).toBe('main_menu');

    // Down twice -> cursor on "Version Information", then confirm.
    core.step(Pad.Down);
    for (let i = 0; i < 6; i += 1) core.step(0);
    core.step(Pad.Down);
    for (let i = 0; i < 6; i += 1) core.step(0);
    core.step(Pad.Cross);
    for (let i = 0; i < 6; i += 1) core.step(0);
    expect(core.state().screen).toBe('version_info');

    // Cancel returns to the main menu.
    core.step(Pad.Circle);
    for (let i = 0; i < 6; i += 1) core.step(0);
    expect(core.state().screen).toBe('main_menu');
  });

  it('applies boot history to the tower field and survives malformed values', () => {
    core.setMode(SceneMode.Idle);
    core.setBootHistory(new Array<number>(21).fill(0));
    core.step(0);
    expect(core.state().active_towers).toBe(0);

    core.setBootHistory(new Array<number>(21).fill(4));
    core.step(0);
    expect(core.state().active_towers).toBe(126);

    // Out-of-range and short inputs must be clamped, never crash.
    core.setBootHistory([999, -5, 12.7, Number.NaN]);
    core.step(0);
    expect(core.state().active_towers).toBeGreaterThan(0);
  });

  it('rejects garbage asset imports instead of corrupting state', () => {
    const garbage = new Uint8Array(4096);
    for (let i = 0; i < garbage.length; i += 1) garbage[i] = (i * 37) & 0xff;
    expect(core.importRomdir(garbage)).toBe(0);
    expect(core.expandLzss(garbage)).toBeNull();
    expect(core.state().asset_imported).toBe(false);
    // Rejects an empty buffer too.
    expect(core.importRomdir(new Uint8Array(0))).toBe(0);
  });

  it('survives aggressive screen churn without panicking (regression: non-ASCII glyph index)', () => {
    core.init(7, false);
    core.setMode(SceneMode.Menu);
    const buttons = [0, Pad.Down, Pad.Down, Pad.Up, Pad.Cross, Pad.Circle, Pad.Left, Pad.Right, Pad.Triangle];
    // Walk every menu screen: main menu -> config -> each editor -> version info -> browser.
    for (let round = 0; round < 3; round += 1) {
      for (const button of buttons) {
        core.step(button);
        for (let i = 0; i < 4; i += 1) core.step(0);
      }
    }
    const screens = new Set<string>();
    for (let i = 0; i < 200; i += 1) {
      core.step(buttons[i % buttons.length]!);
      screens.add(core.state().screen);
    }
    expect(screens.size).toBeGreaterThan(1);
    expect(core.state().commands).toBeGreaterThan(0);
  });

  it('keeps the persisted state JSON well formed after mode churn', () => {
    for (const mode of [SceneMode.Menu, SceneMode.Illegal, SceneMode.Opening, SceneMode.Idle]) {
      core.setMode(mode);
      core.step(0);
      const state = core.state();
      expect(state.fps_target).toBe(50);
      expect(state.clock).toHaveLength(3);
      expect(state.boot_history).toHaveLength(21);
      expect(typeof state.asset_source).toBe('string');
    }
  });
});
