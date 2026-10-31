import { expect, type Page } from "@playwright/test";

/**
 * Shared helpers for the browser specs.
 *
 * Ground rules the specs follow:
 *  - never assume a GPU. The app is required to *report* what it used
 *    (`body[data-renderer]`), and the compatibility renderer is a supported
 *    path, so most tests exercise whichever renderer the browser brought.
 *  - never assume storage or audio. The UI has to say when they are missing,
 *    and the specs assert that statement instead of pretending they exist.
 *  - wait on observable state (`body[data-boot]`, `body[data-phase]`), not on
 *    sleeps.
 */

export const START_LABEL = "Start OSDSYS";

export type RendererKind = "webgpu" | "canvas2d" | "";

/** Load the app and wait until boot has settled (ready or failed). */
export async function openApp(page: Page): Promise<string[]> {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.goto("/", { waitUntil: "domcontentloaded" });
  const body = page.locator("body");
  await expect(body).toHaveAttribute("data-boot", /^(ready|failed)$/, { timeout: 45_000 });

  // Storage resolves asynchronously after boot; wait for the capability line to
  // stop saying "detecting" so tests never race the settings load.
  if ((await body.getAttribute("data-boot")) === "ready") {
    await expect(page.locator("#capabilities")).toContainText(/settings persist|will not persist/, {
      timeout: 45_000,
    });
  }
  return pageErrors;
}

/** True when the app reports a durable settings store. */
export async function hasDurableStorage(page: Page): Promise<boolean> {
  const text = await page.locator("#capabilities").innerText();
  return /settings persist/.test(text);
}

/** The single activation gesture: one click, nothing else. */
export async function startScene(page: Page): Promise<void> {
  const start = page.getByRole("button", { name: START_LABEL });
  await expect(start).toBeEnabled();
  await start.click();
  await expect(page.locator("body")).toHaveAttribute("data-phase", "running", {
    timeout: 45_000,
  });
  await expect(page.locator("#start")).toBeHidden();
}

export async function rendererKind(page: Page): Promise<RendererKind> {
  const value = await page.locator("body").getAttribute("data-renderer");
  return (value ?? "") as RendererKind;
}

/** True when the browser hands out a real WebGPU adapter. */
export async function hasWebGpuAdapter(page: Page): Promise<boolean> {
  try {
    return await page.evaluate(async () => {
      const gpu = (navigator as Navigator & { gpu?: GPU }).gpu;
      if (!gpu) return false;
      const adapter = await gpu.requestAdapter();
      return adapter !== null;
    });
  } catch {
    return false;
  }
}

export async function openPanel(page: Page, label: "Diagnostics" | "Settings" | "Menu") {
  const isMenu = label === "Menu";
  const button = page.getByRole("button", { name: label });
  if ((await button.getAttribute("aria-expanded")) !== "true") await button.click();
  return isMenu ? page.getByRole("menu") : page.locator(`#${label.toLowerCase()}`);
}

/** Read the diagnostics list as a label -> value map. */
export async function diagnostics(page: Page): Promise<Record<string, string>> {
  await openPanel(page, "Diagnostics");
  const list = page.locator("#diagnostics-list");
  await expect(list).toContainText("simulation frame");
  return list.evaluate((dl) => {
    const out: Record<string, string> = {};
    const terms = Array.from(dl.querySelectorAll("dt"));
    const defs = Array.from(dl.querySelectorAll("dd"));
    terms.forEach((dt, i) => {
      out[(dt.textContent ?? "").trim()] = (defs[i]?.textContent ?? "").trim();
    });
    return out;
  });
}

export function numberFrom(text: string | undefined, fallback = Number.NaN): number {
  const match = (text ?? "").match(/-?\d+(?:\.\d+)?/);
  return match ? Number(match[0]) : fallback;
}
