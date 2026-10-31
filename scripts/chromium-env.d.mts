import type { LaunchOptions } from "@playwright/test";

export function preparePlaywrightChromiumOptions(): Promise<{
  launchOptions: LaunchOptions;
}>;
