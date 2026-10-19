// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { keepsSpace } from "./keys";

function el(html: string): HTMLElement {
  document.body.innerHTML = html;
  const target = document.body.querySelector("[data-target]");
  if (!(target instanceof HTMLElement)) throw new Error("no target");
  return target;
}

describe("keepsSpace", () => {
  it("leaves Space to text fields", () => {
    expect(keepsSpace(el('<input data-target type="text">'))).toBe(true);
    expect(keepsSpace(el('<input data-target type="search">'))).toBe(true);
    expect(keepsSpace(el("<textarea data-target></textarea>"))).toBe(true);
  });

  it("takes Space from controls in the window", () => {
    expect(keepsSpace(el("<button data-target>Play</button>"))).toBe(false);
    expect(keepsSpace(el('<input data-target type="range">'))).toBe(false);
    expect(keepsSpace(el('<span data-target role="slider" tabindex="0"></span>'))).toBe(false);
    expect(keepsSpace(el("<select data-target></select>"))).toBe(false);
    expect(keepsSpace(document.body)).toBe(false);
    expect(keepsSpace(null)).toBe(false);
  });

  it("leaves Space to controls in dialogs and menus", () => {
    expect(keepsSpace(el('<div role="dialog"><button data-target>OK</button></div>'))).toBe(true);
    expect(keepsSpace(el('<div role="menu"><div data-target role="menuitem"></div></div>'))).toBe(
      true,
    );
  });
});
