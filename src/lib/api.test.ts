import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
const convertFileSrc = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
const open = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke, convertFileSrc }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));

import {
  closeBook,
  createBook,
  inspectFolder,
  lastBook,
  onIngestProgress,
  openBook,
  pageUrl,
  pickFolder,
  saveProgress,
} from "./api";

const BOOK = { id: "b", title: "B", root: "/r", lastReadPage: 0, saveSeq: 0, pages: [] };
const CMD_ERR = { kind: "ChecksumMismatch", message: "damaged" };

beforeEach(() => {
  vi.resetAllMocks();
});

describe("commands", () => {
  it("inspectFolder sends { path }", async () => {
    const info = { kind: "book", id: "b", title: "B", pages: 2 };
    invoke.mockResolvedValue(info);
    await expect(inspectFolder("/x")).resolves.toBe(info);
    expect(invoke).toHaveBeenCalledWith("inspect_folder", { path: "/x" });
  });

  it("createBook sends folder, title, id, rights, attribution and nothing else", async () => {
    invoke.mockResolvedValue(BOOK);
    const args = { folder: "/f", title: "T", id: "t", rights: "R", attribution: "A" };
    await expect(createBook({ ...args, extra: 1 } as typeof args)).resolves.toBe(BOOK);
    expect(invoke).toHaveBeenCalledWith("create_book", args);
  });

  it("openBook sends { path }", async () => {
    invoke.mockResolvedValue(BOOK);
    await expect(openBook("/p")).resolves.toBe(BOOK);
    expect(invoke).toHaveBeenCalledWith("open_book", { path: "/p" });
  });

  it("saveProgress sends { root, page }", async () => {
    invoke.mockResolvedValue("saved");
    await expect(saveProgress("/r", 4)).resolves.toBe("saved");
    const first = invoke.mock.calls[invoke.mock.calls.length - 1][1] as { seq: number };
    expect(invoke).toHaveBeenCalledWith("save_progress", { root: "/r", page: 4, seq: first.seq });
    await saveProgress("/r", 5);
    const second = invoke.mock.calls[invoke.mock.calls.length - 1][1] as { seq: number };
    expect(second.seq).toBeGreaterThan(first.seq);
  });

  it("numbers saves above a reopened book's own save number", async () => {
    invoke.mockResolvedValue({ ...BOOK, saveSeq: 500 });
    await lastBook();
    invoke.mockResolvedValue("saved");
    await saveProgress("/r", 1);
    const sent = invoke.mock.calls[invoke.mock.calls.length - 1][1] as { seq: number };
    expect(sent.seq).toBe(501);
  });

  it("closeBook sends { root }", async () => {
    invoke.mockResolvedValue(undefined);
    await expect(closeBook("/r")).resolves.toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("close_book", { root: "/r" });
  });

  it("lastBook sends no arguments and may return null", async () => {
    invoke.mockResolvedValue(null);
    await expect(lastBook()).resolves.toBeNull();
    expect(invoke).toHaveBeenCalledWith("last_book", undefined);
  });
});

describe("error normalization", () => {
  it("passes a CommandError rejection through", async () => {
    invoke.mockRejectedValue(CMD_ERR);
    await expect(openBook("/p")).rejects.toBe(CMD_ERR);
  });

  it.each([
    ["inspectFolder", () => inspectFolder("/x")],
    ["createBook", () => createBook({ folder: "", title: "", id: "", rights: "", attribution: "" })],
    ["openBook", () => openBook("/p")],
    ["saveProgress", () => saveProgress("/r", 0)],
    ["lastBook", () => lastBook()],
  ])("%s turns other rejections into Unknown", async (_name, run) => {
    invoke.mockRejectedValue("ipc failed");
    await expect(run()).rejects.toEqual({ kind: "Unknown", message: "ipc failed" });
  });
});

describe("onIngestProgress", () => {
  it("listens on ingest-progress, unwraps the payload, and returns unlisten", async () => {
    const unlisten = vi.fn();
    listen.mockResolvedValue(unlisten);
    const cb = vi.fn();
    await expect(onIngestProgress(cb)).resolves.toBe(unlisten);
    expect(listen.mock.calls[0][0]).toBe("ingest-progress");
    listen.mock.calls[0][1]({ payload: { done: 1, total: 5 } });
    expect(cb).toHaveBeenCalledWith({ done: 1, total: 5 });
  });

  it("normalizes a failure to listen", async () => {
    listen.mockRejectedValue(new Error("no"));
    await expect(onIngestProgress(vi.fn())).rejects.toEqual({
      kind: "Unknown",
      message: "Error: no",
    });
  });
});

describe("pickFolder", () => {
  it("asks for a single directory and returns the path", async () => {
    open.mockResolvedValue("/chosen");
    await expect(pickFolder()).resolves.toBe("/chosen");
    expect(open).toHaveBeenCalledWith({ directory: true, multiple: false });
  });

  it("returns null when cancelled", async () => {
    open.mockResolvedValue(null);
    await expect(pickFolder()).resolves.toBeNull();
  });

  it("normalizes a dialog failure", async () => {
    open.mockRejectedValue("denied");
    await expect(pickFolder()).rejects.toEqual({ kind: "Unknown", message: "denied" });
  });
});

describe("pageUrl", () => {
  it("delegates to convertFileSrc", () => {
    convertFileSrc.mockReturnValue("asset://localhost/x");
    expect(pageUrl("/x")).toBe("asset://localhost/x");
    expect(convertFileSrc).toHaveBeenCalledWith("/x");
  });
});
