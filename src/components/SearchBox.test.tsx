// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { SEARCH_DEBOUNCE_MS, SearchBox } from "./SearchBox";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("SearchBox", () => {
  it("calls onSearch once, 50 ms after the last keystroke", () => {
    vi.useFakeTimers();
    const onSearch = vi.fn();
    render(<SearchBox onSearch={onSearch} />);
    const input = screen.getByRole("searchbox");

    fireEvent.change(input, { target: { value: "d" } });
    act(() => vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS - 10));
    fireEvent.change(input, { target: { value: "deep" } });
    act(() => vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS - 1));
    expect(onSearch).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(1));
    expect(onSearch).toHaveBeenCalledTimes(1);
    expect(onSearch).toHaveBeenCalledWith("deep");
  });

  it("does not search on mount", () => {
    vi.useFakeTimers();
    const onSearch = vi.fn();
    render(<SearchBox onSearch={onSearch} />);
    act(() => vi.advanceTimersByTime(1000));
    expect(onSearch).not.toHaveBeenCalled();
  });
});
