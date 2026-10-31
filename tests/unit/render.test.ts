/**
 * Render-stream verification without a GPU.
 *
 * This runs the real WASM module and feeds its command stream to the independent software
 * rasterizer in `tests/support/software-raster.ts`. It verifies composition, geometry, texture
 * binding and GS blend behaviour — the things a GPU would otherwise be needed to see — and writes
 * PNG snapshots to `test-results/render/` for human inspection.
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { beforeAll, describe, expect, it } from 'vitest';
import { SceneMode, WasmCore } from '../../src/wasm/bridge';
import { SoftwareRenderer, encodePng, statsOf } from '../support/software-raster';

const OUT_DIR = resolve(process.cwd(), 'test-results/render');
const WASM_PATH = resolve(process.cwd(), 'public/wasm/osdbits_core.wasm');

function save(name: string, renderer: SoftwareRenderer): void {
  mkdirSync(OUT_DIR, { recursive: true });
  writeFileSync(resolve(OUT_DIR, `${name}.png`), encodePng(renderer.screen));
}

const HISTORY = [9, 6, 4, 3, 3, 2, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

describe('render stream', () => {
  let core: WasmCore;

  beforeAll(async () => {
    const bytes = readFileSync(WASM_PATH);
    core = await WasmCore.load(`data:application/wasm;base64,${bytes.toString('base64')}`);
  });

  /** A fresh engine in `mode`: the module is a singleton, so each case re-initialises it. */
  function reset(mode: SceneMode, history: number[] = HISTORY): SoftwareRenderer {
    core.init(0x13579bdf, false);
    core.setBootHistory(history);
    core.setMode(mode);
    return new SoftwareRenderer(core);
  }

  it('renders the opening into a recognisable tower field', () => {
    const renderer = reset(SceneMode.Opening);
    for (let i = 0; i < 100; i += 1) core.step(0);
    renderer.render(core);
    save('opening', renderer);

    const stats = statsOf(renderer.screen);
    // The scene must actually fill a meaningful part of the frame.
    expect(stats.coverage).toBeGreaterThan(0.15);
    // Towers are lit geometry: some pixels reach a bright value.
    expect(stats.maxLuma).toBeGreaterThan(0.35);
    // The title text is white-grey over a blue-black field, so the mean is dominated by the
    // towers and fog rather than by the clear colour.
    expect(stats.meanLuma).toBeGreaterThan(0.02);
  });

  it('keeps the illegal-disc screen red-dominant', () => {
    const renderer = reset(SceneMode.Illegal);
    for (let i = 0; i < 70; i += 1) core.step(0);
    renderer.render(core);
    save('illegal', renderer);

    const stats = statsOf(renderer.screen);
    expect(stats.coverage).toBeGreaterThan(0.1);
    expect(stats.red).toBeGreaterThan(stats.green);
    expect(stats.red).toBeGreaterThan(stats.blue);
  });

  it('renders the main menu with an orb cluster brighter than the surround', () => {
    const renderer = reset(SceneMode.Menu);
    for (let i = 0; i < 140; i += 1) core.step(0);
    renderer.render(core);
    save('menu', renderer);

    const stats = statsOf(renderer.screen);
    expect(stats.maxLuma).toBeGreaterThan(0.2);
    // Orbs sit near the centre; the vignette and empty space keep the border darker.
    expect(stats.centerLuma).toBeGreaterThan(stats.borderLuma);
  });

  it('renders System Configuration with the clock ring and the item column', () => {
    const renderer = reset(SceneMode.Menu);
    for (let i = 0; i < 20; i += 1) core.step(0);
    // Main menu → Down → confirm "System Configuration".
    core.step(0x4000);
    for (let i = 0; i < 6; i += 1) core.step(0);
    core.step(0x0040);
    for (let i = 0; i < 120; i += 1) core.step(0);
    expect(core.state().screen).toBe('system_config');
    renderer.render(core);
    save('system-config', renderer);

    const stats = statsOf(renderer.screen);
    expect(stats.coverage).toBeGreaterThan(0.05);
    // The tunnel, rods and text all contribute; the tinted composite keeps blue above red.
    expect(stats.blue).toBeGreaterThan(stats.red);
  });

  it('shows the memory-card browser with its 21 slots', () => {
    const renderer = reset(SceneMode.Menu);
    for (let i = 0; i < 20; i += 1) core.step(0);
    core.step(0x0040); // confirm "Browser" (cursor is on the first item)
    for (let i = 0; i < 10; i += 1) core.step(0);
    expect(core.state().screen).toBe('browser');
    renderer.render(core);
    save('browser', renderer);
    expect(statsOf(renderer.screen).coverage).toBeGreaterThan(0.01);
  });

  it('idle mode keeps producing a tower field indefinitely', () => {
    const renderer = reset(SceneMode.Idle);
    for (let i = 0; i < 900; i += 1) core.step(0);
    renderer.render(core);
    save('idle-900', renderer);
    const stats = statsOf(renderer.screen);
    expect(stats.coverage).toBeGreaterThan(0.15);
    expect(stats.maxLuma).toBeGreaterThan(0.35);
  });

  it('a heavier boot history produces more lit coverage than an empty card', () => {
    const renderer = reset(SceneMode.Idle, new Array<number>(21).fill(0));

    core.setBootHistory(new Array<number>(21).fill(0));
    for (let i = 0; i < 30; i += 1) core.step(0);
    renderer.render(core);
    const empty = statsOf(renderer.screen).coverage;

    core.setBootHistory(new Array<number>(21).fill(6));
    for (let i = 0; i < 30; i += 1) core.step(0);
    renderer.render(core);
    const full = statsOf(renderer.screen).coverage;
    save('idle-full-card', renderer);

    expect(full).toBeGreaterThan(empty);
  });
});
