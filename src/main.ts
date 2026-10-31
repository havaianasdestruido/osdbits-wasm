import { OsdbitsAppController } from './app/controller';

export const APP_VERSION = '0.1.0';

declare global {
  interface Window {
    __OSDBITS_APP__?: OsdbitsAppController;
  }
}

function bootstrapApp(): void {
  if (typeof document === 'undefined') {
    return;
  }
  const canvas = document.getElementById('osdbits-canvas');
  if (!canvas) {
    const legacyRoot = document.getElementById('app');
    if (legacyRoot) {
      legacyRoot.dataset['version'] = APP_VERSION;
    }
    return;
  }

  const controller = new OsdbitsAppController(document);
  if (typeof window !== 'undefined') {
    window.__OSDBITS_APP__ = controller;
    window.addEventListener('pagehide', () => {
      controller.dispose();
    });
  }
  void controller.bootstrapPreflight();
}

bootstrapApp();
