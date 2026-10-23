// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { clearThumbCache } from "../useArtworkUrl";
import { Artwork } from "./Artwork";

const { artworkGet, artworkThumb } = vi.hoisted(() => ({
  artworkGet: vi.fn(),
  artworkThumb: vi.fn(),
}));
vi.mock("../../lib/ipc", () => ({ artworkGet, artworkThumb }));

beforeEach(() => {
  URL.createObjectURL = vi.fn(() => "blob:art");
  URL.revokeObjectURL = vi.fn();
  artworkGet.mockResolvedValue(new ArrayBuffer(4));
  artworkThumb.mockResolvedValue(new ArrayBuffer(4));
});

afterEach(() => {
  cleanup();
  clearThumbCache();
  vi.clearAllMocks();
});

describe("Artwork", () => {
  it("shows the placeholder without artwork and asks for nothing", () => {
    render(<Artwork size="row" artworkId={null} />);
    expect(screen.getByRole("img", { name: "No artwork" })).toBeTruthy();
    expect(artworkGet).not.toHaveBeenCalled();
    expect(artworkThumb).not.toHaveBeenCalled();
  });

  it("falls back to the placeholder when the picture can't be read", async () => {
    artworkGet.mockRejectedValue(new Error("gone"));
    render(<Artwork artworkId={5} mime="image/png" />);
    await waitFor(() => expect(artworkGet).toHaveBeenCalledWith(5));
    expect(screen.getByRole("img", { name: "No artwork" })).toBeTruthy();
  });

  it("loads the full picture for the info panel", async () => {
    render(<Artwork artworkId={5} mime="image/png" />);
    await waitFor(() => expect(screen.getByAltText("Artwork")).toBeTruthy());
    expect(artworkThumb).not.toHaveBeenCalled();
  });

  it("loads the small thumbnail for a list row, once per picture", async () => {
    const { container } = render(
      <>
        <Artwork size="row" artworkId={7} />
        <Artwork size="row" artworkId={7} />
      </>,
    );
    await waitFor(() => expect(container.querySelectorAll("img")).toHaveLength(2));
    expect(artworkThumb).toHaveBeenCalledTimes(1);
    expect(artworkGet).not.toHaveBeenCalled();
  });
});
