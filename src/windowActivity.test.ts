import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const bridge = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  onActivity: undefined as undefined | ((event: { payload: boolean }) => void),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: bridge.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: bridge.listen }));

let onVisibilityChange: () => void;
let fakeDocument: {
  visibilityState: string;
  documentElement: { dataset: Record<string, string> };
  getAnimations: ReturnType<typeof vi.fn>;
  addEventListener: ReturnType<typeof vi.fn>;
};

beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers();
  bridge.onActivity = undefined;
  bridge.invoke.mockReset().mockResolvedValue(false);
  bridge.listen.mockReset().mockImplementation(async (_event, handler) => {
    bridge.onActivity = handler;
    return () => undefined;
  });
  fakeDocument = {
    visibilityState: "visible",
    documentElement: { dataset: {} },
    getAnimations: vi.fn(() => []),
    addEventListener: vi.fn((_event, handler) => { onVisibilityChange = handler; }),
  };
  vi.stubGlobal("document", fakeDocument);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("window activity", () => {
  it("stops hidden clocks and catches up immediately on every reopen", async () => {
    const activity = await import("./windowActivity");
    await activity.initializeWindowActivity("detail");
    const ticks = vi.fn(() => Date.now());
    const stop = activity.startActiveInterval(ticks, 1000);
    vi.advanceTimersByTime(5000);
    expect(ticks).not.toHaveBeenCalled();

    bridge.onActivity!({ payload: true });
    expect(ticks).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(2000);
    expect(ticks).toHaveBeenCalledTimes(3);
    bridge.onActivity!({ payload: false });
    vi.advanceTimersByTime(60_000);
    expect(ticks).toHaveBeenCalledTimes(3);
    expect(vi.getTimerCount()).toBe(0);

    bridge.onActivity!({ payload: true });
    expect(ticks.mock.results.at(-1)?.value).toBe(Date.now());
    bridge.onActivity!({ payload: true });
    expect(vi.getTimerCount()).toBe(1);
    stop();
    expect(vi.getTimerCount()).toBe(0);
    bridge.onActivity!({ payload: false });
    bridge.onActivity!({ payload: true });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("does not overwrite a newer show event with a stale initial query", async () => {
    let resolveInitial!: (visible: boolean) => void;
    bridge.invoke.mockImplementation(() => new Promise<boolean>((resolve) => { resolveInitial = resolve; }));
    const activity = await import("./windowActivity");
    const initialized = activity.initializeWindowActivity("detail");
    await Promise.resolve();
    bridge.onActivity!({ payload: true });
    resolveInitial(false);
    await initialized;
    expect(fakeDocument.documentElement.dataset.windowActive).toBe("true");
  });

  it("keeps native-hidden windows paused through browser visibility changes", async () => {
    const activity = await import("./windowActivity");
    await activity.initializeWindowActivity("detail");
    onVisibilityChange();
    expect(fakeDocument.documentElement.dataset.windowActive).toBe("false");
    bridge.onActivity!({ payload: true });
    fakeDocument.visibilityState = "hidden";
    onVisibilityChange();
    expect(fakeDocument.documentElement.dataset.windowActive).toBe("false");
    fakeDocument.visibilityState = "visible";
    onVisibilityChange();
    expect(fakeDocument.documentElement.dataset.windowActive).toBe("true");
  });

  it("resumes only animations paused by the window lifecycle", async () => {
    const running = { playState: "running", pause: vi.fn(() => { running.playState = "paused"; }), play: vi.fn(() => { running.playState = "running"; }) };
    const alreadyPaused = { playState: "paused", pause: vi.fn(), play: vi.fn() };
    fakeDocument.getAnimations.mockReturnValue([running, alreadyPaused]);
    const activity = await import("./windowActivity");
    await activity.initializeWindowActivity("detail");
    bridge.onActivity!({ payload: true });
    bridge.onActivity!({ payload: false });
    expect(running.pause).toHaveBeenCalledTimes(1);
    expect(alreadyPaused.pause).not.toHaveBeenCalled();
    bridge.onActivity!({ payload: true });
    expect(running.play).toHaveBeenCalledTimes(1);
    expect(alreadyPaused.play).not.toHaveBeenCalled();
  });

  it("keeps a natively visible taskbar running when the page reports hidden", async () => {
    bridge.invoke.mockResolvedValue(true);
    fakeDocument.visibilityState = "hidden";
    const activity = await import("./windowActivity");
    await activity.initializeWindowActivity("taskbar");
    const ticks = vi.fn(() => Date.now());
    const stop = activity.startActiveInterval(ticks, 1000);

    expect(fakeDocument.documentElement.dataset.windowActive).toBe("true");
    vi.advanceTimersByTime(5000);
    expect(ticks).toHaveBeenCalledTimes(6);
    onVisibilityChange();
    bridge.onActivity!({ payload: true });
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(1000);
    expect(ticks).toHaveBeenCalledTimes(7);
    stop();
  });

  it("still pauses the taskbar when it is actually hidden and catches up on restore", async () => {
    bridge.invoke.mockResolvedValue(true);
    fakeDocument.visibilityState = "hidden";
    const activity = await import("./windowActivity");
    await activity.initializeWindowActivity("taskbar");
    const ticks = vi.fn(() => Date.now());
    const stop = activity.startActiveInterval(ticks, 1000);
    expect(ticks).toHaveBeenCalledTimes(1);

    bridge.onActivity!({ payload: false });
    fakeDocument.visibilityState = "visible";
    onVisibilityChange();
    vi.advanceTimersByTime(60_000);
    expect(ticks).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);

    fakeDocument.visibilityState = "hidden";
    onVisibilityChange();
    bridge.onActivity!({ payload: true });
    expect(ticks).toHaveBeenCalledTimes(2);
    expect(ticks.mock.results.at(-1)?.value).toBe(Date.now());
    expect(vi.getTimerCount()).toBe(1);
    stop();
  });

  it.each(["detail", "settings", "menu"])("keeps the %s popup paused when hidden", async (label) => {
    const activity = await import("./windowActivity");
    await activity.initializeWindowActivity(label);
    const ticks = vi.fn();
    const stop = activity.startActiveInterval(ticks, 1000);
    vi.advanceTimersByTime(60_000);
    expect(ticks).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);

    bridge.onActivity!({ payload: true });
    fakeDocument.visibilityState = "hidden";
    onVisibilityChange();
    expect(fakeDocument.documentElement.dataset.windowActive).toBe("false");
    expect(vi.getTimerCount()).toBe(0);
    stop();
  });
});
