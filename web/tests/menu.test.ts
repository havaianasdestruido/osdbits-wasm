import { describe, expect, it } from "vitest";
import { CLOSED, MENU_ITEMS, activate, close, move, open, selected, toFirst, toLast } from "../src/menu.js";

describe("menu model", () => {
  it("opens on the first item and closes to a clean state", () => {
    const s = open();
    expect(s.open).toBe(true);
    expect(s.index).toBe(0);
    expect(close()).toEqual(CLOSED);
  });

  it("wraps in both directions", () => {
    let s = open();
    s = move(s, -1);
    expect(s.index).toBe(MENU_ITEMS.length - 1);
    s = move(s, 1);
    expect(s.index).toBe(0);
    s = move(s, 3);
    expect(s.index).toBe(3 % MENU_ITEMS.length);
  });

  it("supports Home/End", () => {
    const s = open(2);
    expect(toFirst(s).index).toBe(0);
    expect(toLast(s).index).toBe(MENU_ITEMS.length - 1);
  });

  it("ignores navigation while closed", () => {
    const s = move(CLOSED, 2);
    expect(s).toEqual(CLOSED);
    expect(selected(CLOSED)).toBeNull();
  });

  it("activates the selected action and closes", () => {
    const s = open(2);
    const result = activate(s);
    expect(result.action).toBe(MENU_ITEMS[2].id);
    expect(result.state.open).toBe(false);
  });

  it("clamps an out-of-range open index", () => {
    expect(open(99).index).toBe(MENU_ITEMS.length - 1);
    expect(open(-5).index).toBe(0);
  });

  it("has unique, well-formed items", () => {
    const ids = new Set(MENU_ITEMS.map((i) => i.id));
    expect(ids.size).toBe(MENU_ITEMS.length);
    for (const item of MENU_ITEMS) {
      expect(item.label.length).toBeGreaterThan(0);
      expect(item.id.length).toBeGreaterThan(0);
    }
  });
});
