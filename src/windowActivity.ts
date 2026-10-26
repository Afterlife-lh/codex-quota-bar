import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

let nativeVisible = false;
let active = false;
let nativeVisibilityOnly = false;
const subscribers = new Set<() => void>();
const pausedAnimations = new Set<Animation>();

function updateActivity() {
  // A transparent, non-activating taskbar window can be classified as a hidden
  // page while its native window is still on screen. Only popups use that extra
  // browser visibility gate; the resident widget follows its own native window.
  const next = nativeVisible && (nativeVisibilityOnly || document.visibilityState !== "hidden");
  document.documentElement.dataset.windowActive = String(next);
  if (next === active) return;
  active = next;

  // The CSS rule handles CSS animations, including ones mounted while hidden.
  // Also pause Web Animations API effects such as the theme transition.
  if (active) {
    for (const animation of pausedAnimations) {
      if (animation.playState === "paused") animation.play();
    }
    pausedAnimations.clear();
  } else {
    for (const animation of document.getAnimations()) {
      if (animation.playState === "running") {
        animation.pause();
        pausedAnimations.add(animation);
      }
    }
  }
  for (const notify of subscribers) notify();
}

function setNativeVisible(visible: boolean) {
  nativeVisible = visible;
  updateActivity();
}

export async function initializeWindowActivity(windowLabel: string) {
  nativeVisibilityOnly = windowLabel === "taskbar";
  document.documentElement.dataset.windowActive = "false";
  document.addEventListener("visibilitychange", updateActivity);
  let revision = 0;
  await listen<boolean>("window-activity-changed", ({ payload }) => {
    revision += 1;
    setNativeVisible(payload);
  });
  const initialRevision = revision;
  const visible = await invoke<boolean>("get_window_activity");
  // A show/hide event may arrive while the initial native query is in flight.
  if (revision === initialRevision) setNativeVisible(visible);
}

function subscribe(notify: () => void) {
  subscribers.add(notify);
  return () => { subscribers.delete(notify); };
}

export function useWindowActive() {
  return useSyncExternalStore(subscribe, () => active);
}

export function startActiveInterval(callback: () => void, milliseconds: number) {
  let timer: ReturnType<typeof setInterval> | undefined;
  const sync = () => {
    if (timer !== undefined) clearInterval(timer);
    timer = undefined;
    if (active) {
      callback();
      timer = setInterval(callback, milliseconds);
    }
  };
  const unsubscribe = subscribe(sync);
  sync();
  return () => {
    unsubscribe();
    if (timer !== undefined) clearInterval(timer);
  };
}
