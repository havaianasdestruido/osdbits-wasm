/**
 * OSDSYS-inspired menu model (mode D).
 *
 * Pure navigation state so it can be tested without a DOM. This is a
 * *reconstruction-inspired* menu, not a reconstruction of OSDSYS's browser,
 * memory-card or system-configuration screens (upstream only has notes for
 * those; see docs/LIMITATIONS.md and TODO.MD `D-005`).
 */

export type MenuAction =
  | "scene:boot"
  | "scene:idle"
  | "scene:illegal"
  | "settings"
  | "about"
  | "close";

export interface MenuItem {
  id: MenuAction;
  label: string;
  hint?: string;
}

export const MENU_ITEMS: readonly MenuItem[] = [
  { id: "scene:boot", label: "Restart the opening", hint: "tower field, camera drift, fly-up" },
  { id: "scene:idle", label: "Idle tower field", hint: "no exit condition" },
  { id: "scene:illegal", label: "Illegal-disc scene", hint: "red cloud, flare, fades" },
  { id: "settings", label: "Settings", hint: "quality, effects, audio" },
  { id: "about", label: "About this reconstruction" },
  { id: "close", label: "Close menu" },
];

export interface MenuState {
  open: boolean;
  index: number;
}

export const CLOSED: MenuState = { open: false, index: 0 };

/** Open the menu at the first item (or at `index` when valid). */
export function open(index = 0): MenuState {
  const clamped = Math.min(Math.max(0, index), MENU_ITEMS.length - 1);
  return { open: true, index: clamped };
}

export function close(): MenuState {
  return { ...CLOSED };
}

/** Move the cursor by `delta`, wrapping at both ends. */
export function move(state: MenuState, delta: number): MenuState {
  if (!state.open || MENU_ITEMS.length === 0) return state;
  const n = MENU_ITEMS.length;
  const index = (((state.index + delta) % n) + n) % n;
  return { ...state, index };
}

export function toFirst(state: MenuState): MenuState {
  return state.open ? { ...state, index: 0 } : state;
}

export function toLast(state: MenuState): MenuState {
  return state.open ? { ...state, index: MENU_ITEMS.length - 1 } : state;
}

/** The action a confirm would trigger, or null when nothing is selected. */
export function selected(state: MenuState): MenuAction | null {
  if (!state.open) return null;
  return MENU_ITEMS[state.index]?.id ?? null;
}

/** Apply a confirm; returns the action and the state it leaves behind. */
export function activate(state: MenuState): { action: MenuAction | null; state: MenuState } {
  const action = selected(state);
  return { action, state: action === null ? state : { ...state, open: false } };
}
