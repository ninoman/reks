// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import type { MixerLook } from "../../lib/mixView";
import { useMix } from "../../store/mix";
import { ShortcutHelp } from "./ShortcutHelp";

const initial = useMix.getState();
afterEach(() => {
  cleanup();
  useMix.setState(initial, true);
});

function open(look: MixerLook) {
  useMix.setState({ look });
  render(<ShortcutHelp />);
  fireEvent.click(screen.getByRole("button", { name: "Shortcuts" }));
}

describe("ShortcutHelp", () => {
  it("lists X C V only where the mixer has a crossfader", () => {
    for (const [look, shown] of [
      [{ eq: "fourBand", view: "advanced", filters: "show" }, true],
      [{ eq: "fourBand", view: "simple", filters: "show" }, false],
      [{ eq: "threeBand", view: "advanced", filters: "show" }, true],
      [{ eq: "threeBand", view: "simple", filters: "show" }, false],
    ] as const) {
      open(look);
      expect(screen.queryByText("Crossfader left, centre, right") !== null).toBe(shown);
      cleanup();
    }
  });
});
