/**
 * Entry point. Installs the failure paths that must work before anything else
 * (uncaught errors, unhandled rejections), then boots the app.
 */

import { App } from "./app.js";

function fatal(message: string): void {
  const target = document.getElementById("banner-text");
  const banner = document.getElementById("banner");
  if (target && banner) {
    target.textContent = message;
    banner.hidden = false;
  }
  console.error(message);
}

window.addEventListener("error", (event) => {
  fatal(`Unexpected error: ${event.message}`);
});
window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason instanceof Error ? event.reason.message : String(event.reason);
  fatal(`Unhandled error: ${reason}`);
});

const app = new App();
void app.boot().catch((error: unknown) => {
  fatal(`The app failed to boot: ${error instanceof Error ? error.message : String(error)}`);
});
