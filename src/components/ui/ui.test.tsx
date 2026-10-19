// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Button } from "./Button";
import { Checkbox } from "./Checkbox";
import type { ControlSize } from "./control";
import { Dialog, DialogContent, DialogTitle } from "./Dialog";
import { IconButton } from "./IconButton";
import { Input } from "./Input";
import { Segment, SegmentedControl } from "./SegmentedControl";
import { Select } from "./Select";
import { Switch } from "./Switch";

afterEach(cleanup);

const SIZES: ControlSize[] = ["xs", "sm", "md", "lg"];

/** The height utility on an element (`h-(--ctl-h-sm)` etc.). */
function heightClass(el: Element): string | undefined {
  return [...el.classList].find((c) => c.startsWith("h-"));
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
