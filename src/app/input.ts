/**
 * Keyboard/gamepad → physical pad word mapping.
 *
 * The core takes the *physical* pad word (0x0020 = physical Circle, 0x0040 = physical Cross) and
 * performs the ROM's region swap itself (`pad.c` / `docs/pad.md`), so the same key is "confirm"
 * on a Japanese-region run and "cancel" on a World one — exactly like the console.
 */

import { Pad } from '../wasm/bridge';

const KEY_MAP: Readonly<Record<string, number>> = {
  ArrowUp: Pad.Up,
  ArrowDown: Pad.Down,
  ArrowLeft: Pad.Left,
  ArrowRight: Pad.Right,
  KeyW: Pad.Up,
  KeyS: Pad.Down,
  KeyA: Pad.Left,
  KeyD: Pad.Right,
  Enter: Pad.Cross,
  NumpadEnter: Pad.Cross,
  Space: Pad.Cross,
  Escape: Pad.Circle,
  Backspace: Pad.Circle,
  KeyX: Pad.Cross,
  KeyO: Pad.Circle,
  KeyZ: Pad.Square,
  KeyT: Pad.Triangle,
  KeyQ: Pad.L1,
  KeyE: Pad.R1,
  ShiftLeft: Pad.L2,
  ShiftRight: Pad.R2,
  Tab: Pad.Select,
};

export class InputController {
  private held = new Set<string>();
  private gamepadButtons = 0;

  constructor(private readonly target: Window = window) {
    this.target.addEventListener('keydown', this.onKeyDown, { passive: false });
    this.target.addEventListener('keyup', this.onKeyUp);
    this.target.addEventListener('blur', this.onBlur);
  }

  private onKeyDown = (event: KeyboardEvent): void => {
    const code = event.code;
    if (KEY_MAP[code] === undefined && !['Tab'].includes(code)) return;
    // Tab is used by the panel; only swallow the pad keys.
    if (KEY_MAP[code] !== undefined) event.preventDefault();
    this.held.add(code);
  };

  private onKeyUp = (event: KeyboardEvent): void => {
    this.held.delete(event.code);
  };

  private onBlur = (): void => {
    this.held.clear();
  };

  /** Polls connected gamepads once per frame and folds them into the pad word. */
  pollGamepads(): void {
    this.gamepadButtons = 0;
    const pads = navigator.getGamepads?.() ?? [];
    for (const pad of pads) {
      if (!pad) continue;
      const b = pad.buttons;
      const pressed = (index: number): boolean => Boolean(b[index]?.pressed);
      const axisX = pad.axes[0] ?? 0;
      const axisY = pad.axes[1] ?? 0;
      if (pressed(0)) this.gamepadButtons |= Pad.Cross; // A / Cross
      if (pressed(1)) this.gamepadButtons |= Pad.Circle; // B / Circle
      if (pressed(2)) this.gamepadButtons |= Pad.Square; // X / Square
      if (pressed(3)) this.gamepadButtons |= Pad.Triangle; // Y / Triangle
      if (pressed(4) || pressed(6)) this.gamepadButtons |= Pad.L1;
      if (pressed(5) || pressed(7)) this.gamepadButtons |= Pad.R1;
      if (pressed(8)) this.gamepadButtons |= Pad.Select;
      if (pressed(9)) this.gamepadButtons |= Pad.Start;
      if (pressed(12) || axisY < -0.5) this.gamepadButtons |= Pad.Up;
      if (pressed(13) || axisY > 0.5) this.gamepadButtons |= Pad.Down;
      if (pressed(14) || axisX < -0.5) this.gamepadButtons |= Pad.Left;
      if (pressed(15) || axisX > 0.5) this.gamepadButtons |= Pad.Right;
      break;
    }
  }

  /** The active-high physical pad word for this frame. */
  word(): number {
    let word = this.gamepadButtons;
    for (const code of this.held) {
      const bit = KEY_MAP[code];
      if (bit !== undefined) word |= bit;
    }
    return word >>> 0;
  }

  /** Presses a virtual button for one frame (used by the UI buttons and the tests). */
  pressVirtual(bit: number): number {
    return (this.word() | bit) >>> 0;
  }

  dispose(): void {
    this.target.removeEventListener('keydown', this.onKeyDown);
    this.target.removeEventListener('keyup', this.onKeyUp);
    this.target.removeEventListener('blur', this.onBlur);
    this.held.clear();
  }
}
