// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { deckKeyHint } from "../../lib/mixKeys";
import { Button } from "./Button";
import { Checkbox } from "./Checkbox";
import { ChoiceTile, ChoiceTiles } from "./ChoiceTiles";
import type { ControlSize } from "./control";
import { Dialog, DialogContent, DialogTitle } from "./Dialog";
import { IconButton } from "./IconButton";
import { InlineNumber, parseNumber } from "./InlineNumber";
import { Input } from "./Input";
import { LampButton } from "./LampButton";
import { Pad, SplitPad } from "./Pad";
import { Readout } from "./Readout";
import { Segment, SegmentedControl } from "./SegmentedControl";
import { Select } from "./Select";
import { Switch } from "./Switch";

afterEach(cleanup);

const SIZES: ControlSize[] = ["xs", "sm", "md", "lg"];

/** The height utility on an element (`h-(--ctl-h-sm)` etc.); `min-h-` (controls that may wrap) counts as `h-`. */
function heightClass(el: Element): string | undefined {
  return [...el.classList].find((c) => /^(min-)?h-/.test(c))?.replace(/^min-/, "");
}

describe("control size scale", () => {
  it.each(SIZES)("%s: every control has the same height token", (size) => {
    render(
      <div>
        <Button size={size}>B</Button>
        <IconButton size={size} aria-label="I">
          i
        </IconButton>
        <Input size={size} aria-label="input" />
        <Input size={size} variant="search" aria-label="search" />
        <Select size={size} aria-label="select">
          <option>a</option>
        </Select>
        <SegmentedControl size={size} value="a" onValueChange={() => {}} aria-label="seg">
          <Segment value="a">A</Segment>
        </SegmentedControl>
      </div>,
    );
    const controls = [
      screen.getByRole("button", { name: "B" }),
      screen.getByRole("button", { name: "I" }),
      screen.getByRole("textbox", { name: "input" }),
      screen.getByRole("searchbox", { name: "search" }),
      screen.getByRole("combobox", { name: "select" }),
      screen.getByRole("radiogroup", { name: "seg" }),
    ];
    expect(controls.map(heightClass)).toEqual(controls.map(() => `h-(--ctl-h-${size})`));
  });

  it("icon buttons are square", () => {
    render(
      <IconButton size="sm" aria-label="I">
        i
      </IconButton>,
    );
    expect(screen.getByRole("button").classList).toContain("w-(--ctl-h-sm)");
  });

  it("className is merged last", () => {
    render(<Button className="px-0">B</Button>);
    const cls = [...screen.getByRole("button").classList];
    expect(cls).toContain("px-0");
    expect(cls.some((c) => c.startsWith("px-(--ctl"))).toBe(false);
  });
});

describe("shortcut hints", () => {
  it("a button with a shortcut shows it on focus, from the key map", async () => {
    const key = deckKeyHint({ type: "play" }, 1);
    render(
      <Button aria-label="Play deck 2" hint="Play" shortcut={key}>
        ▶
      </Button>,
    );
    fireEvent.focus(screen.getByRole("button", { name: "Play deck 2" }));
    const tip = await screen.findByRole("tooltip");
    expect(tip.textContent).toContain("Play");
    expect(tip.textContent).toContain("Z");
  });

  it("an icon button's hint falls back to its label", async () => {
    render(<IconButton aria-label="Eject" shortcut="E" />);
    fireEvent.focus(screen.getByRole("button", { name: "Eject" }));
    expect((await screen.findByRole("tooltip")).textContent).toContain("Eject");
  });

  it("no shortcut, no tooltip", () => {
    render(<Button>Plain</Button>);
    fireEvent.focus(screen.getByRole("button", { name: "Plain" }));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});

describe("Switch", () => {
  it("is a switch that reports its new state", () => {
    const onChange = vi.fn();
    render(<Switch aria-label="Apply" checked={false} onCheckedChange={onChange} />);
    const sw = screen.getByRole("switch", { name: "Apply" });
    expect(sw.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(sw);
    expect(onChange).toHaveBeenCalledWith(true);
  });
});

describe("Checkbox", () => {
  it("is a native checkbox named by its label", () => {
    const onChange = vi.fn();
    render(
      <label>
        <Checkbox checked={false} onChange={(e) => onChange(e.target.checked)} />I understand
      </label>,
    );
    fireEvent.click(screen.getByRole("checkbox", { name: "I understand" }));
    expect(onChange).toHaveBeenCalledWith(true);
  });
});

describe("SegmentedControl", () => {
  it("is a radio group; picking the current segment keeps it", () => {
    const onChange = vi.fn();
    render(
      <SegmentedControl value="a" onValueChange={onChange} aria-label="seg">
        <Segment value="a">A</Segment>
        <Segment value="b">B</Segment>
      </SegmentedControl>,
    );
    expect(screen.getByRole("radio", { name: "A" }).getAttribute("aria-checked")).toBe("true");
    fireEvent.click(screen.getByRole("radio", { name: "B" }));
    expect(onChange).toHaveBeenLastCalledWith("b");
    fireEvent.click(screen.getByRole("radio", { name: "A" }));
    expect(onChange).toHaveBeenCalledTimes(1);
  });
});

describe("ChoiceTiles", () => {
  it("is a radio group of tiles named by their label and described by their hint", () => {
    const onChange = vi.fn();
    render(
      <ChoiceTiles value="a" onValueChange={onChange} aria-label="tiles">
        <ChoiceTile value="a" label="Alpha" hint="The first" isDefault preview={<span />} />
        <ChoiceTile value="b" label="Beta" hint="The second" />
      </ChoiceTiles>,
    );
    expect(screen.getByRole("radiogroup", { name: "tiles" })).toBeTruthy();
    const alpha = screen.getByRole("radio", { name: "Alpha" });
    expect(alpha.getAttribute("aria-checked")).toBe("true");
    expect(alpha.textContent).toContain("Default");
    const beta = screen.getByRole("radio", { name: "Beta" });
    const hint = document.getElementById(beta.getAttribute("aria-describedby") ?? "");
    expect(hint?.textContent).toBe("The second");
    fireEvent.click(beta);
    expect(onChange).toHaveBeenLastCalledWith("b");
    fireEvent.click(alpha);
    expect(onChange).toHaveBeenCalledTimes(1);
  });
});

describe("Dialog", () => {
  it("Escape asks to close", () => {
    const onOpenChange = vi.fn();
    render(
      <Dialog open onOpenChange={onOpenChange}>
        <DialogContent>
          <DialogTitle>Title</DialogTitle>
          <button type="button">ok</button>
        </DialogContent>
      </Dialog>,
    );
    expect(screen.getByRole("dialog", { name: "Title" })).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});

describe("lit switches", () => {
  it("a control button's pressed state is the lit fill, not the selected tint", () => {
    render(
      <Button variant="control" aria-pressed>
        Cue
      </Button>,
    );
    const cls = screen.getByRole("button").classList;
    expect(cls).toContain("aria-pressed:bg-lit");
    expect(cls).toContain("aria-pressed:shadow-lit");
    expect(cls).not.toContain("aria-pressed:bg-selected");
    expect(cls).toContain("focus-visible:outline-focus-ring");
  });

  it("a lit control icon button keeps the lit text colour (light lit fills)", () => {
    render(
      <IconButton variant="control" aria-pressed aria-label="Play">
        P
      </IconButton>,
    );
    const cls = screen.getByRole("button").classList;
    expect(cls).toContain("aria-pressed:text-lit-fg");
    expect(cls).not.toContain("aria-pressed:text-selected-fg");
  });

  it("a control segmented control marks itself for its lit segments", () => {
    render(
      <SegmentedControl variant="control" value="a" onValueChange={() => {}} aria-label="seg">
        <Segment value="a">A</Segment>
      </SegmentedControl>,
    );
    expect(screen.getByRole("radiogroup").getAttribute("data-variant")).toBe("control");
  });
});

describe("Pad", () => {
  it("is at least --pad-h tall and fills its cell, in every tone", () => {
    render(
      <>
        <Pad aria-label="a">A</Pad>
        <Pad aria-label="b" tone="set" color="#ffffff">
          B
        </Pad>
        <Pad aria-label="c" tone="lit" marked>
          C
        </Pad>
      </>,
    );
    for (const name of ["a", "b", "c"]) {
      expect(screen.getByRole("button", { name }).classList).toContain("min-h-(--pad-h)");
    }
  });

  it("a data colour outlines it, with no fill", () => {
    render(<Pad aria-label="cue" tone="set" color="#f0a020" />);
    const pad = screen.getByRole("button", { name: "cue" });
    expect(pad.style.getPropertyValue("--pad-color")).toBe("#f0a020");
    expect(pad.classList).toContain("bg-transparent");
  });

  it("a split pad is two buttons under one label", () => {
    const back = vi.fn();
    const forward = vi.fn();
    render(
      <SplitPad
        label="4"
        backLabel="Back 4"
        forwardLabel="Forward 4"
        onBack={back}
        onForward={forward}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Back 4" }));
    fireEvent.click(screen.getByRole("button", { name: "Forward 4" }));
    expect(back).toHaveBeenCalledTimes(1);
    expect(forward).toHaveBeenCalledTimes(1);
  });
});

describe("InlineNumber", () => {
  const setup = (value: number | null = 128) => {
    const commit = vi.fn();
    render(
      <InlineNumber
        value={value}
        format={(v) => v.toFixed(2)}
        onCommit={commit}
        aria-label="BPM"
      />,
    );
    return commit;
  };

  it("double-click opens a field; Enter commits", () => {
    const commit = setup();
    fireEvent.doubleClick(screen.getByRole("button", { name: "BPM" }));
    const field = screen.getByRole("textbox", { name: "BPM" }) as HTMLInputElement;
    expect(field.value).toBe("128.00");
    fireEvent.change(field, { target: { value: "131,5" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(commit).toHaveBeenCalledWith(131.5);
    expect(screen.getByRole("button", { name: "BPM" })).toBeTruthy();
  });

  it("Enter on the focused text opens it; Esc cancels and blur commits", () => {
    const commit = setup();
    fireEvent.keyDown(screen.getByRole("button", { name: "BPM" }), { key: "Enter" });
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "120" } });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Escape" });
    expect(commit).not.toHaveBeenCalled();
    fireEvent.doubleClick(screen.getByRole("button", { name: "BPM" }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "122" } });
    fireEvent.blur(screen.getByRole("textbox"));
    expect(commit).toHaveBeenCalledWith(122);
  });

  it("drops text that is not a number, and does not open without a value", () => {
    const commit = setup();
    fireEvent.doubleClick(screen.getByRole("button", { name: "BPM" }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "fast" } });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    expect(commit).not.toHaveBeenCalled();
    cleanup();
    setup(null);
    expect((screen.getByRole("button", { name: "BPM" }) as HTMLButtonElement).disabled).toBe(true);
    expect(parseNumber(" 99.5 ")).toBe(99.5);
    expect(parseNumber("1e3")).toBeNull();
  });
});

describe("Readout", () => {
  it("shows its value under a full name, dimmed when not in effect", () => {
    render(
      <div>
        <Readout aria-label="Loop size: 4">4</Readout>
        <Readout dimmed aria-label="Next loop size: 1/8">
          1/8
        </Readout>
      </div>,
    );
    const on = screen.getByRole("img", { name: "Loop size: 4" });
    const dim = screen.getByRole("img", { name: "Next loop size: 1/8" });
    expect(on.textContent).toBe("4");
    expect(on.dataset.dimmed).toBeUndefined();
    expect(on.className).toContain("text-fg");
    expect(dim.dataset.dimmed).toBe("true");
    expect(dim.className).toContain("text-fg-subtle");
  });

  it("is one height with the buttons of its size", () => {
    render(
      <div>
        <Readout size="xs" aria-label="r">
          1
        </Readout>
        <Button size="xs">b</Button>
      </div>,
    );
    expect(heightClass(screen.getByRole("img"))).toBe(
      heightClass(screen.getByRole("button", { name: "b" })),
    );
  });
});

describe("LampButton", () => {
  it("is a button that reports and toggles its pressed state", () => {
    const onClick = vi.fn();
    render(<LampButton aria-label="Lamp" pressed={false} onClick={onClick} />);
    const lamp = screen.getByRole("button", { name: "Lamp" });
    expect(lamp.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(lamp);
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});
