// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ConvertDryRun, ConvertEvent, ConvertReportView } from "../lib/ipc";

vi.mock("../lib/ipc", async (original) => ({
  ...(await original<typeof import("../lib/ipc")>()),
  convertPlan: vi.fn(),
  convertApply: vi.fn(),
  convertCancel: vi.fn(),
  pickFolder: vi.fn(),
  libraryQuery: vi.fn(),
  libraryFacets: vi.fn(),
  playlistsGet: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const convertPlan = vi.mocked(ipc.convertPlan);
const convertApply = vi.mocked(ipc.convertApply);
const convertCancel = vi.mocked(ipc.convertCancel);
const pickFolder = vi.mocked(ipc.pickFolder);
vi.mocked(ipc.libraryQuery).mockResolvedValue({ rows: [], total: 0 } as never);
vi.mocked(ipc.libraryFacets).mockResolvedValue({
  genres: [],
  keys: [],
  colours: [],
  playlists: [],
} as never);
vi.mocked(ipc.playlistsGet).mockResolvedValue({ nodes: [] } as never);
const { DEFAULT_SETTINGS, useConverter } = await import("../store/converter");
const { ConvertDialog } = await import("./ConvertDialog");

const DRY_RUN: ConvertDryRun = {
  planId: 3,
  ready: true,
  status: "Ready to convert 1 file. Nothing has been written yet.",
  facts: [{ label: "Size", value: "41.2 MB → about 23.0 MB" }],
  items: [
    {
      trackId: 2,
      label: "A – Two",
      source: "/m/two.flac",
      dest: "/m/two.aiff",
      from: "FLAC · 24-bit · 96 kHz · stereo",
      to: "AIFF · 24-bit · 48 kHz · stereo",
      sizeText: "41.2 MB → 23.0 MB",
    },
  ],
  skipped: [{ trackId: 3, label: "B – Three", reason: "MP3 or AAC: only lossless files convert" }],
  warnings: ["A – Two: imported from rekordbox"],
};

const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;

beforeEach(() => {
  vi.clearAllMocks();
  useConverter.setState({ step: "closed", hidden: false, settings: DEFAULT_SETTINGS });
});
afterEach(cleanup);

describe("ConvertDialog", () => {
  it("walks settings → dry run → progress → report", async () => {
    convertPlan.mockResolvedValue(DRY_RUN);
    const run = { send: (_: ConvertEvent) => {}, finish: (_: ConvertReportView) => {} };
    convertApply.mockImplementation((_id, onEvent) => {
      run.send = (e) => act(() => onEvent(e));
      return new Promise((resolve) => {
        run.finish = resolve;
      });
    });
    convertCancel.mockResolvedValue();
    useConverter.getState().open([2, 3]);
    render(<ConvertDialog />);

    expect(screen.getByText("2 tracks", { exact: false })).toBeTruthy();
    expect(screen.getByRole("radio", { name: "Older players" }).getAttribute("data-state")).toBe(
      "on",
    );
    fireEvent.click(screen.getByRole("radio", { name: "Save space" }));
    expect((screen.getByLabelText("Format") as HTMLSelectElement).value).toBe("flac");
    fireEvent.change(screen.getByLabelText("Format"), { target: { value: "wav" } });
    expect(screen.getByText("Custom settings.")).toBeTruthy();
    fireEvent.click(screen.getByRole("radio", { name: "Older players" }));

    // Trash is only for Replace.
    fireEvent.click(screen.getByRole("radio", { name: "Add as new" }));
    expect(
      (screen.getByRole("radio", { name: "Move to Trash" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    fireEvent.click(screen.getByRole("radio", { name: "Replace" }));

    fireEvent.click(button("Dry run"));
    expect((await screen.findByRole("status")).textContent).toMatch(/Ready to convert 1 file/);
    expect(convertPlan).toHaveBeenCalledWith(
      [2, 3],
      { target: "aiff", depth: "keep", rate: "keep" },
      { destination: "nextToOriginal", library: "replace", originals: "keep", capRate48k: true },
    );
    expect(screen.getByText(/FLAC · 24-bit · 96 kHz · stereo → AIFF/)).toBeTruthy();
    expect(screen.getByText("/m/two.aiff")).toBeTruthy();
    expect(screen.getByText("B – Three: MP3 or AAC: only lossless files convert")).toBeTruthy();
    expect(screen.getByText("A – Two: imported from rekordbox")).toBeTruthy();

    fireEvent.click(button("Convert"));
    run.send({ type: "started", trackId: 2, index: 0, total: 1 });
    expect(screen.getByText("File 1 of 1: A – Two")).toBeTruthy();
    fireEvent.click(button("Cancel"));
    await waitFor(() => expect(convertCancel).toHaveBeenCalledTimes(1));
    expect(button("Stopping…").disabled).toBe(true);

    await act(async () =>
      run.finish({
        outcome: "cancelled",
        title: "Conversion cancelled",
        status: "Converted 0 of 1 file. Files not started are unchanged.",
        facts: [],
        converted: [],
        failed: [],
        skipped: [],
      }),
    );
    expect(await screen.findByText("Conversion cancelled")).toBeTruthy();
    fireEvent.click(button("Done"));
    expect(useConverter.getState().step).toBe("closed");
  });

  it("chooses a destination folder", async () => {
    pickFolder.mockResolvedValue("/Users/dj/Converted");
    useConverter.getState().open([2]);
    render(<ConvertDialog />);
    fireEvent.click(screen.getByRole("radio", { name: "Chosen folder…" }));
    expect(await screen.findByText("/Users/dj/Converted", { exact: false })).toBeTruthy();
    expect(useConverter.getState().settings.folder).toBe("/Users/dj/Converted");
    fireEvent.click(screen.getByRole("radio", { name: "Next to the original" }));
    expect(useConverter.getState().settings.folder).toBeNull();
  });

  it("hides while converting", async () => {
    convertPlan.mockResolvedValue(DRY_RUN);
    convertApply.mockImplementation(() => new Promise(() => {}));
    useConverter.getState().open([2]);
    render(<ConvertDialog />);
    fireEvent.click(button("Dry run"));
    await screen.findByText(/Ready to convert/);
    fireEvent.click(button("Convert"));
    fireEvent.click(button("Hide"));
    expect(screen.queryByText("Convert files")).toBeNull();
    expect(useConverter.getState()).toMatchObject({ step: "converting", hidden: true });
  });
});
