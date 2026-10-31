// @vitest-environment jsdom
/**
 * Boot integration test.
 *
 * The browser specs in web/playwright are the real thing, but they cannot run
 * in this sandbox (no browser binaries). This test boots the actual app module
 * against the real index.html inside jsdom with the real wasm module, so the
 * wiring — DOM ids, storage fallback, capability reporting, the activation
 * gesture — is exercised on every local run too.
 *
 * What it cannot cover: layout, WebGPU, Canvas 2D, audio output. It asserts
 * only what jsdom can honestly observe.
 */

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";

// vitest runs with the web/ package as its root.
const html = readFileSync(resolve(process.cwd(), "index.html"), "utf8");
const wasmPath = resolve(process.cwd(), "public/osdbits_core.wasm");

/** Install the real markup (minus the module script) into the jsdom document. */
function installMarkup(): void {
  const body = html.slice(html.indexOf("<body>") + "<body>".length, html.indexOf("</body>"));
  // The skip link and the module script are irrelevant here.
  document.body.innerHTML = body.replace(/<script[\s\S]*?<\/script>/g, "");
}

/** Minimal Response-ish object for the wasm fetch the app performs. */
function wasmResponse(bytes: Uint8Array): Response {
  return {
    ok: true,
    status: 200,
    statusText: "OK",
    headers: new Headers({ "content-type": "application/wasm" }),
    arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
  } as unknown as Response;
}

async function bootApp() {
  const { App } = await import("../src/app.js");
  const app = new App();
  await app.boot();
  return app;
}

describe("boot", () => {
  beforeEach(() => {
    installMarkup();
    vi.stubGlobal(
      "matchMedia",
      vi.fn().mockImplementation((query: string) => ({
        matches: false,
        media: query,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
        addListener: () => undefined,
        removeListener: () => undefined,
        onchange: null,
        dispatchEvent: () => false,
      })),
    );
    const bytes = new Uint8Array(readFileSync(wasmPath));
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => wasmResponse(bytes)),
    );
  });

  it("reaches the ready state with the real wasm module", async () => {
    const app = await bootApp();
    expect(document.body.dataset.boot).toBe("ready");
    expect(document.body.dataset.phase).toBe("idle");

    const start = document.getElementById("startBtn") as HTMLButtonElement;
    expect(start).not.toBeNull();
    expect(start.disabled).toBe(false);

    // Capability reporting must be honest about what jsdom does not have.
    const capabilities = document.getElementById("capabilities")?.textContent ?? "";
    expect(capabilities).toMatch(/WebGPU (available|unavailable)/);
    expect(capabilities).toMatch(/settings persist|will not persist/);
    expect(app).toBeTruthy();
  });

  it("keeps the start overlay up until the gesture happens", async () => {
    await bootApp();
    const overlay = document.getElementById("start") as HTMLElement;
    expect(overlay.hidden).toBe(false);
    expect(document.getElementById("toolbar")).not.toBeNull();
    expect(document.getElementById("status")?.textContent).toBe("idle");
  });

  it("reports a core that fails to load instead of pretending", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: false, status: 404, statusText: "Not Found" }) as unknown as Response),
    );
    const app = await bootApp();
    expect(document.body.dataset.boot).toBe("failed");
    const start = document.getElementById("startBtn") as HTMLButtonElement;
    expect(start.disabled).toBe(true);
    const banner = document.getElementById("banner") as HTMLElement;
    expect(banner.hidden).toBe(false);
    expect(document.getElementById("banner-text")?.textContent).toMatch(/simulation core failed to load/i);
    expect(app).toBeTruthy();
  });

  it("keeps the core-failure message when storage resolves late", async () => {
    // Regression: applySettingsToUi() -> setCapabilities() used to overwrite the
    // "core unavailable" line once the (slower) storage lookup finished, which
    // showed up as a browser-specific E2E failure.
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: false, status: 404, statusText: "Not Found" }) as unknown as Response),
    );
    await bootApp();
    const capabilities = () => document.getElementById("capabilities")?.textContent ?? "";
    expect(capabilities()).toMatch(/simulation core is unavailable/i);
    // Let the storage chain finish and settle.
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(capabilities()).toMatch(/simulation core is unavailable/i);
  });

  it("applies the reduced-motion preference before the first frame", async () => {
    vi.stubGlobal(
      "matchMedia",
      vi.fn().mockImplementation((query: string) => ({
        matches: query.includes("prefers-reduced-motion"),
        media: query,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
        addEventListener_: () => undefined,
        onchange: null,
        dispatchEvent: () => false,
      })),
    );
    await bootApp();
    expect(document.documentElement.dataset.cameraMotion).toBe("off");
    // The settings form is rendered once the stored settings resolve.
    await vi.waitFor(() => {
      const reduced = document.getElementById("field-reducedMotion") as HTMLInputElement | null;
      expect(reduced?.checked).toBe(true);
    });
  });

  it("lists the menu items and wires the settings form", async () => {
    await bootApp();
    const items = document.querySelectorAll("#menu-items [role='menuitem']");
    expect(items.length).toBeGreaterThan(0);

    const settings = document.getElementById("settingsBtn") as HTMLButtonElement;
    settings.click();
    const panel = document.getElementById("settings") as HTMLElement;
    expect(panel.hidden).toBe(false);

    const quality = document.getElementById("field-quality") as HTMLSelectElement;
    expect(quality).not.toBeNull();
    expect(quality.value).toBe("high");
    const seed = document.getElementById("field-seed") as HTMLInputElement;
    expect(seed.value).toBe("4242");

    quality.value = "low";
    const form = document.getElementById("settings-form") as HTMLFormElement;
    form.dispatchEvent(new Event("submit", { cancelable: true, bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(document.getElementById("settings-status")?.textContent).toMatch(/saved|applied/i);
  });
});
