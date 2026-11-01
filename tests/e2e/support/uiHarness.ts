/**
 * Shared helpers for UILess e2e specs. The page has no DOM controls: every in-app control is
 * drawn on the canvas, so tests drive it through the same pointer and keyboard path a user does.
 */

import { expect, type Page } from "@playwright/test";

export async function waitForAppReady(page: Page): Promise<void> {
  await page.waitForFunction(() => document.getElementById("app")?.dataset.appState === "ready", null, {
    timeout: 15_000,
  });
}

async function nextFrame(page: Page): Promise<void> {
  await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
}

/** Client-space centre of a drawn, enabled UI region, or null when it is not clickable now. */
async function uiPoint(page: Page, id: string): Promise<{ x: number; y: number; enabled: boolean } | null> {
  return page.evaluate((target) => window.__osdbitsHarness?.app.uiHitCenter(target) ?? null, id);
}

/**
 * Clicks an in-app region with a real mouse click on the canvas. Waits until the region is
 * drawn and enabled (menu rows are gated during the core's transitions). On the options screen
 * a missing row is reached by moving the selection down, which also scrolls the list.
 */
export async function clickUi(page: Page, id: string, timeoutMs = 20_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const point = await uiPoint(page, id);
    if (point?.enabled) {
      await page.mouse.click(point.x, point.y);
      return;
    }
    const screen = await page.locator("#app").getAttribute("data-ui-screen");
    if (point === null && screen === "options") {
      await page.keyboard.press("ArrowDown");
    }
    await nextFrame(page);
  }
  throw new Error(`UI region "${id}" never became clickable`);
}

export async function uiRegionVisible(page: Page, id: string): Promise<boolean> {
  return (await uiPoint(page, id)) !== null;
}

export async function drawnTextIncludes(page: Page, needle: string): Promise<boolean> {
  return page.evaluate((text) => {
    const lower = text.toLowerCase();
    return (window.__osdbitsHarness?.app.getDrawnUiText() ?? []).some((line) =>
      line.toLowerCase().includes(lower),
    );
  }, needle);
}

/** Asserts the page is only the application: one canvas, a hidden status line, no controls. */
export async function expectOnlyApplicationVisible(page: Page): Promise<void> {
  const report = await page.evaluate(() => {
    const bodyChildren = Array.from(document.body.children).map((el) => el.tagName);
    const app = document.getElementById("app");
    const appChildren = app
      ? Array.from(app.children).map((el) => ({
          tag: el.tagName,
          visibleChrome: !el.classList.contains("sr-only") && el.tagName !== "CANVAS",
          hidden: el.hasAttribute("hidden"),
        }))
      : [];
    const controls = document.querySelectorAll(
      "button, input, select, textarea, label, a[href], nav, header, footer, aside, section, dialog, [role=dialog], [role=toolbar], [role=menu], [role=tablist], [role=navigation]",
    ).length;
    return {
      bodyChildren,
      appChildren,
      controls,
      canvasLabel: document.getElementById("osdbits-canvas")?.getAttribute("aria-label") ?? null,
    };
  });
  expect(report.bodyChildren.filter((tag) => tag !== "SCRIPT")).toEqual(["DIV"]);
  const chrome = report.appChildren.filter((c) => c.visibleChrome && !c.hidden);
  expect(chrome).toEqual([]);
  expect(report.appChildren.map((c) => c.tag)).toContain("CANVAS");
  expect(report.controls).toBe(0);
  expect(report.canvasLabel).toBeTruthy();
}

export async function sceneMenuScreen(page: Page): Promise<string | null> {
  return page.evaluate(() => window.__osdbitsHarness?.app.getLatestSnapshot()?.menuUi?.screen ?? null);
}
