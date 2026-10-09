import { useEffect, useRef } from "react";

const ACTIVITY = ["pointerdown", "keydown", "wheel", "touchstart"] as const;

/** Calls `onLock` after `minutes` without input; 0 disables it (setting security.auto_lock_minutes). */
export function useIdleLock(minutes: number, onLock: () => void): void {
  const callback = useRef(onLock);
  useEffect(() => {
    callback.current = onLock;
  }, [onLock]);
  useEffect(() => {
    if (minutes <= 0) return undefined;
    const delay = minutes * 60_000;
    let timer = window.setTimeout(() => callback.current(), delay);
    const reset = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => callback.current(), delay);
    };
    for (const name of ACTIVITY) window.addEventListener(name, reset, { passive: true });
    return () => {
      window.clearTimeout(timer);
      for (const name of ACTIVITY) window.removeEventListener(name, reset);
    };
  }, [minutes]);
}
