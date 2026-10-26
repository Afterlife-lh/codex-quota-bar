import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { QuotaSnapshot } from "./types";

/** Reconcile with Rust's cached snapshot, never an extra OpenAI request. */
export function subscribeQuota(onSnapshot: (snapshot: QuotaSnapshot) => void) {
  let disposed = false;
  let revision = 0;
  let unlisten: UnlistenFn | undefined;
  const eventsReady = listen<QuotaSnapshot>("quota-updated", ({ payload }) => {
    if (disposed) return;
    revision += 1;
    onSnapshot(payload);
  }).then((stop) => {
    if (disposed) stop();
    else unlisten = stop;
  }).catch(() => undefined);

  return {
    async refresh() {
      const requestRevision = ++revision;
      // Subscribe first so no update can fall between the read and listener.
      await eventsReady;
      if (disposed || requestRevision !== revision) return;
      try {
        const snapshot = await invoke<QuotaSnapshot>("get_status");
        // An in-flight cache read must not overwrite a newer pushed snapshot,
        // including errors with the same last-success timestamp.
        if (!disposed && requestRevision === revision) onSnapshot(snapshot);
      } catch {
        // Keep the last known data if the local IPC bridge is unavailable.
      }
    },
    dispose() {
      disposed = true;
      revision += 1;
      unlisten?.();
      unlisten = undefined;
    },
  };
}
