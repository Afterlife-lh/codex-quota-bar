import { beforeEach, describe, expect, it, vi } from "vitest";
import type { QuotaSnapshot } from "./types";

const bridge = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  unlisten: vi.fn(),
  onQuota: undefined as undefined | ((event: { payload: QuotaSnapshot }) => void),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: bridge.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: bridge.listen }));

import { subscribeQuota } from "./quotaSubscription";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

function snapshot(remaining: number, queriedAt = 1000): QuotaSnapshot {
  return {
    windows: [{ kind: "seven_day", label: "7 天", usedPercent: 100 - remaining, remainingPercent: remaining }],
    queriedAt,
    cached: false,
    credentialStatus: "valid",
  };
}

beforeEach(() => {
  bridge.onQuota = undefined;
  bridge.unlisten.mockReset();
  bridge.invoke.mockReset().mockResolvedValue(snapshot(80));
  bridge.listen.mockReset().mockImplementation(async (_event, handler) => {
    bridge.onQuota = handler;
    return bridge.unlisten;
  });
});

describe("quota cache reconciliation", () => {
  it("subscribes before reading the cached status without requesting a remote refresh", async () => {
    const registration = deferred<() => void>();
    bridge.listen.mockReturnValue(registration.promise);
    const update = vi.fn();
    const source = subscribeQuota(update);
    const read = source.refresh();
    expect(bridge.listen).toHaveBeenCalledWith("quota-updated", expect.any(Function));
    expect(bridge.invoke).not.toHaveBeenCalled();
    registration.resolve(bridge.unlisten);
    await read;
    expect(bridge.invoke).toHaveBeenCalledTimes(1);
    expect(bridge.invoke).toHaveBeenCalledWith("get_status");
    expect(update).toHaveBeenCalledWith(snapshot(80));
    source.dispose();
  });

  it("does not overwrite a newer pushed quota with an older in-flight read", async () => {
    const cached = deferred<QuotaSnapshot>();
    bridge.invoke.mockReturnValue(cached.promise);
    const update = vi.fn();
    const source = subscribeQuota(update);
    const read = source.refresh();
    await vi.waitFor(() => expect(bridge.invoke).toHaveBeenCalledTimes(1));
    bridge.onQuota!({ payload: snapshot(74, 2000) });
    cached.resolve(snapshot(80));
    await read;
    expect(update).toHaveBeenCalledTimes(1);
    expect(update).toHaveBeenLastCalledWith(snapshot(74, 2000));
    source.dispose();
  });

  it("also preserves an error push with an unchanged last-success timestamp", async () => {
    const cached = deferred<QuotaSnapshot>();
    bridge.invoke.mockReturnValue(cached.promise);
    const update = vi.fn();
    const source = subscribeQuota(update);
    const read = source.refresh();
    await vi.waitFor(() => expect(bridge.invoke).toHaveBeenCalledTimes(1));
    const failure = { ...snapshot(80), cached: true, error: "请求超时" };
    bridge.onQuota!({ payload: failure });
    cached.resolve(snapshot(80));
    await read;
    expect(update).toHaveBeenCalledTimes(1);
    expect(update).toHaveBeenCalledWith(failure);
    source.dispose();
  });

  it("uses only the latest of overlapping resume/cache requests", async () => {
    const oldRead = deferred<QuotaSnapshot>();
    const newRead = deferred<QuotaSnapshot>();
    bridge.invoke.mockReturnValueOnce(oldRead.promise).mockReturnValueOnce(newRead.promise);
    const update = vi.fn();
    const source = subscribeQuota(update);
    const first = source.refresh();
    await vi.waitFor(() => expect(bridge.invoke).toHaveBeenCalledTimes(1));
    const second = source.refresh();
    await vi.waitFor(() => expect(bridge.invoke).toHaveBeenCalledTimes(2));
    newRead.resolve(snapshot(74, 2000));
    await second;
    oldRead.resolve(snapshot(80));
    await first;
    expect(update).toHaveBeenCalledTimes(1);
    expect(update).toHaveBeenCalledWith(snapshot(74, 2000));
    source.dispose();
  });

  it("cleans up a pending subscription once and ignores late pushes and reads", async () => {
    const registration = deferred<() => void>();
    bridge.listen.mockImplementation((_event, handler) => {
      bridge.onQuota = handler;
      return registration.promise;
    });
    const update = vi.fn();
    const source = subscribeQuota(update);
    const read = source.refresh();
    source.dispose();
    source.dispose();
    registration.resolve(bridge.unlisten);
    await read;
    bridge.onQuota!({ payload: snapshot(74) });
    expect(update).not.toHaveBeenCalled();
    expect(bridge.invoke).not.toHaveBeenCalled();
    expect(bridge.unlisten).toHaveBeenCalledTimes(1);
  });

  it("retains the last pushed value when the local bridge read fails", async () => {
    const update = vi.fn();
    const source = subscribeQuota(update);
    await source.refresh();
    bridge.onQuota!({ payload: snapshot(74) });
    bridge.invoke.mockRejectedValueOnce(new Error("IPC unavailable"));
    await expect(source.refresh()).resolves.toBeUndefined();
    expect(update).toHaveBeenCalledTimes(2);
    expect(update).toHaveBeenLastCalledWith(snapshot(74));
    source.dispose();
  });

  it("can still reconcile from cache if event subscription fails", async () => {
    bridge.listen.mockRejectedValueOnce(new Error("IPC unavailable"));
    const update = vi.fn();
    const source = subscribeQuota(update);
    await source.refresh();
    expect(update).toHaveBeenCalledTimes(1);
    expect(update).toHaveBeenCalledWith(snapshot(80));
    source.dispose();
  });
});
