/**
 * Mode E: the customisation drawer.
 *
 * Every control is a live binding into the engine — changing a slider calls the WASM setter
 * immediately, so the effect is visible in the very next frame. Persistence is owned by
 * `main.ts`; this module only reports changes through `onChange`.
 */

import type { PersistedSettings } from '../storage/persistence';

export interface PanelCallbacks {
  onChange(settings: PersistedSettings): void;
  onModeChange(mode: number): void;
  onResetSettings(): void;
  onResetBootHistory(): void;
  onImport(): void;
  onResetAssets(): void;
  onExport(): string;
  onImportJson(json: string): string | null;
  getDiagnostics(): string;
}

const MODES: ReadonlyArray<{ value: number; label: string }> = [
  { value: 0, label: 'Mode A — Opening / Towers' },
  { value: 1, label: 'Mode B — Idle tower field' },
  { value: 2, label: 'Mode C — Illegal disc' },
  { value: 3, label: 'Mode D — Main menu & System Configuration' },
];

const LANGUAGES: ReadonlyArray<{ value: number; label: string }> = [
  { value: 1, label: 'English' },
  { value: 2, label: 'Français' },
  { value: 3, label: 'Español' },
  { value: 4, label: 'Deutsch' },
  { value: 5, label: 'Italiano' },
  { value: 6, label: 'Nederlands' },
  { value: 7, label: 'Português' },
];

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, string> = {},
  children: (Node | string)[] = [],
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (key === 'class') node.className = value;
    else node.setAttribute(key, value);
  }
  for (const child of children) {
    node.append(typeof child === 'string' ? document.createTextNode(child) : child);
  }
  return node;
}

export class ControlPanel {
  private readonly root: HTMLElement;
  private readonly body: HTMLElement;
  private settings: PersistedSettings;
  private readonly callbacks: PanelCallbacks;

  constructor(root: HTMLElement, settings: PersistedSettings, callbacks: PanelCallbacks) {
    this.root = root;
    this.settings = settings;
    this.callbacks = callbacks;
    const body = root.querySelector('#panel-body');
    if (!(body instanceof HTMLElement)) throw new Error('Control panel body is missing');
    this.body = body;
    this.render();
  }

  setSettings(settings: PersistedSettings): void {
    this.settings = settings;
    this.render();
  }

  private update(patch: Partial<PersistedSettings>): void {
    this.settings = { ...this.settings, ...patch };
    this.callbacks.onChange(this.settings);
  }

  private slider(
    label: string,
    key: keyof PersistedSettings,
    min: number,
    max: number,
    step: number,
    format: (value: number) => string,
    hint?: string,
  ): HTMLElement {
    const value = Number(this.settings[key]);
    const output = el('span', { class: 'value' }, [format(value)]);
    const input = el('input', {
      type: 'range',
      min: String(min),
      max: String(max),
      step: String(step),
      value: String(value),
      id: `field-${key}`,
    });
    input.addEventListener('input', () => {
      const next = Number(input.value);
      output.textContent = format(next);
      this.update({
        [key]: next,
      } as unknown as Partial<PersistedSettings>);
    });
    const field = el('div', { class: 'field' }, [
      el('label', { for: `field-${key}` }, [label, output]),
      input,
    ]);
    if (hint) field.append(el('span', { class: 'hint' }, [hint]));
    return field;
  }

  private select(
    label: string,
    key: keyof PersistedSettings,
    options: ReadonlyArray<{ value: number; label: string }>,
  ): HTMLElement {
    const select = el('select', { id: `field-${key}` });
    for (const option of options) {
      const node = el('option', { value: String(option.value) }, [option.label]);
      if (Number(this.settings[key]) === option.value) node.selected = true;
      select.append(node);
    }
    select.addEventListener('change', () => {
      this.update({ [key]: Number(select.value) } as unknown as Partial<PersistedSettings>);
    });
    return el('div', { class: 'field' }, [
      el('label', { for: `field-${key}` }, [label]),
      select,
    ]);
  }

  private checkbox(label: string, key: keyof PersistedSettings, hint?: string): HTMLElement {
    const input = el('input', { type: 'checkbox', id: `field-${key}` });
    input.checked = Boolean(this.settings[key]);
    input.addEventListener('change', () => {
      this.update({ [key]: input.checked } as unknown as Partial<PersistedSettings>);
    });
    const field = el('div', { class: 'field' }, [
      el('label', { class: 'checkbox', for: `field-${key}` }, [input, label]),
    ]);
    if (hint) field.append(el('span', { class: 'hint' }, [hint]));
    return field;
  }

  private render(): void {
    this.body.replaceChildren();

    // ---- Scene
    this.body.append(el('div', { class: 'section-title' }, ['Scene']));
    const modeSelect = el('select', { id: 'field-mode' });
    for (const mode of MODES) {
      const option = el('option', { value: String(mode.value) }, [mode.label]);
      if (this.settings.mode === mode.value) option.selected = true;
      modeSelect.append(option);
    }
    modeSelect.addEventListener('change', () => {
      const mode = Number(modeSelect.value);
      this.update({ mode });
      this.callbacks.onModeChange(mode);
    });
    this.body.append(
      el('div', { class: 'field' }, [
        el('label', { for: 'field-mode' }, ['Scene mode']),
        modeSelect,
      ]),
    );
    this.body.append(
      this.slider('Animation speed', 'speed', 0.25, 2, 0.05, (v) => `${v.toFixed(2)}×`),
      this.slider('Fog intensity', 'fogIntensity', 0, 2, 0.05, (v) => `${Math.round(v * 100)}%`),
      this.slider(
        'Glass refraction',
        'refractionScale',
        0,
        2,
        0.05,
        (v) => `${Math.round(v * 100)}%`,
        'Scales the screen-space normal displacement of the refractive cubes.',
      ),
      this.checkbox(
        'Reduce motion',
        'reducedMotion',
        'Slows the camera, disables the zoom blur and softens the light trails.',
      ),
      this.slider('Render scale', 'resolutionScale', 0.5, 2, 0.25, (v) => `${v.toFixed(2)}×`),
    );

    // ---- Appearance
    this.body.append(el('div', { class: 'section-title' }, ['Appearance']));
    this.body.append(this.select('Language', 'language', LANGUAGES));
    this.body.append(
      this.select('Screen size', 'screenSize', [
        { value: 0, label: '4:3' },
        { value: 1, label: 'Full' },
        { value: 2, label: '16:9' },
      ]),
      this.select('Digital out (optical)', 'digitalOut', [
        { value: 0, label: 'Off' },
        { value: 1, label: 'On' },
      ]),
      this.select('Component video out', 'componentOut', [
        { value: 0, label: 'Y Cb/Pb Cr/Pr' },
        { value: 1, label: 'RGB' },
      ]),
      this.checkbox(
        'Japanese region layout',
        'regionJapan',
        'Circle confirms and Cross cancels, and the hint bar swaps to match — the console behaviour for Japanese-region systems.',
      ),
    );

    // ---- Audio
    this.body.append(el('div', { class: 'section-title' }, ['Audio']));
    this.body.append(
      this.slider('Master volume', 'audioMaster', 0, 1, 0.01, (v) => `${Math.round(v * 100)}%`),
      this.slider('Music volume', 'audioBgm', 0, 1, 0.01, (v) => `${Math.round(v * 100)}%`),
      this.slider('Effects volume', 'audioSe', 0, 1, 0.01, (v) => `${Math.round(v * 100)}%`),
      this.checkbox('Mute', 'muted'),
    );

    // ---- Towers / boot history
    this.body.append(el('div', { class: 'section-title' }, ['Tower field']));
    const historyRow = el('div', { class: 'field-row' }, []);
    const presets = el('select', { id: 'field-history-preset' });
    for (const preset of [
      { value: 'fresh', label: 'Fresh console (0 games)' },
      { value: 'light', label: 'Light (4 games)' },
      { value: 'moderate', label: 'Moderate (9 games)' },
      { value: 'full', label: 'Full card (21 games)' },
    ]) {
      presets.append(el('option', { value: preset.value }, [preset.label]));
    }
    presets.addEventListener('change', () => {
      const counts = buildPreset(presets.value);
      this.update({ bootHistory: counts });
    });
    historyRow.append(presets);
    this.body.append(
      el('div', { class: 'field' }, [
        el('label', { for: 'field-history-preset' }, ['Boot history preset']),
        historyRow,
        el('span', { class: 'hint' }, [
          'The 21 memory-card history slots decide which towers exist and how tall they are. Edit them slot by slot in Mode D → Browser.',
        ]),
      ]),
    );
    const resetHistory = el('button', { type: 'button', class: 'ghost' }, ['Clear boot history']);
    resetHistory.addEventListener('click', () => this.callbacks.onResetBootHistory());
    this.body.append(el('div', { class: 'button-row' }, [resetHistory]));

    // ---- Resources
    this.body.append(el('div', { class: 'section-title' }, ['Resources']));
    const importBtn = el('button', { type: 'button', class: 'ghost' }, ['Import BIOS / ROMDIR…']);
    importBtn.addEventListener('click', () => this.callbacks.onImport());
    const resetAssets = el('button', { type: 'button', class: 'ghost' }, ['Use built-in assets']);
    resetAssets.addEventListener('click', () => this.callbacks.onResetAssets());
    this.body.append(el('div', { class: 'button-row' }, [importBtn, resetAssets]));

    // ---- Settings management
    this.body.append(el('div', { class: 'section-title' }, ['Settings']));
    const textarea = el('textarea', { id: 'settings-json', spellcheck: 'false' });
    textarea.value = this.callbacks.onExport();
    const exportBtn = el('button', { type: 'button', class: 'ghost' }, ['Refresh export']);
    exportBtn.addEventListener('click', () => {
      textarea.value = this.callbacks.onExport();
    });
    const copyBtn = el('button', { type: 'button', class: 'ghost' }, ['Copy']);
    copyBtn.addEventListener('click', () => {
      void navigator.clipboard?.writeText(textarea.value);
    });
    const importJsonBtn = el('button', { type: 'button', class: 'ghost' }, ['Import pasted JSON']);
    const jsonStatus = el('span', { class: 'hint' }, []);
    importJsonBtn.addEventListener('click', () => {
      const error = this.callbacks.onImportJson(textarea.value);
      jsonStatus.textContent = error ?? 'Settings imported.';
    });
    const resetBtn = el('button', { type: 'button', class: 'ghost' }, ['Reset to defaults']);
    resetBtn.addEventListener('click', () => {
      if (window.confirm('Reset all settings, boot history and imported assets?')) {
        this.callbacks.onResetSettings();
      }
    });
    this.body.append(
      textarea,
      el('div', { class: 'button-row' }, [exportBtn, copyBtn, importJsonBtn, resetBtn]),
      jsonStatus,
    );

    // ---- Diagnostics
    this.body.append(el('div', { class: 'section-title' }, ['Diagnostics']));
    const diag = el('pre', { class: 'hint' }, [this.callbacks.getDiagnostics()]);
    diag.style.whiteSpace = 'pre-wrap';
    diag.style.margin = '0';
    this.body.append(diag);
  }

  show(): void {
    this.root.hidden = false;
  }

  hide(): void {
    this.root.hidden = true;
  }

  toggle(): void {
    this.root.hidden = !this.root.hidden;
  }

  get isOpen(): boolean {
    return !this.root.hidden;
  }
}

export function buildPreset(name: string): number[] {
  const counts = new Array<number>(21).fill(0);
  switch (name) {
    case 'light':
      for (let i = 0; i < 4; i += 1) counts[i] = 2 + i;
      break;
    case 'moderate':
      for (let i = 0; i < 9; i += 1) counts[i] = 1 + ((i * 3) % 7);
      break;
    case 'full':
      for (let i = 0; i < 21; i += 1) counts[i] = 1 + ((i * 5) % 12);
      break;
    default:
      break;
  }
  return counts;
}
