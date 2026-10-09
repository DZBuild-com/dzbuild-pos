import { useEffect, useState } from "react";
import { commands, events } from "./client";
import type { DisplayFrame, DisplayShop } from "./types";

/** The customer screen's whole state: the last frame the till sent and the shop's branding.
 * One read of each at start, then events — the window never holds a copy of the till's data. */
export function useCustomerFrame(): { frame: DisplayFrame | null; shop: DisplayShop | null } {
  const [frame, setFrame] = useState<DisplayFrame | null>(null);
  const [shop, setShop] = useState<DisplayShop | null>(null);
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    events.customerDisplayUpdate
      .listen((e) => setFrame(e.payload.frame))
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch((error: unknown) => console.warn("customer display", error));
    commands
      .displayCurrent()
      // a frame that came as an event first is newer than this answer
      .then((current) => setFrame((f) => f ?? current))
      .catch((error: unknown) => console.warn("customer display", error));
    commands
      .displayShop()
      .then(setShop)
      .catch((error: unknown) => console.warn("customer display", error));
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);
  return { frame, shop };
}
