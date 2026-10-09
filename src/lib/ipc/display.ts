import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { applyDisplay } from "@/lib/display";
import { commands } from "./client";
import { qk } from "./keys";
import type { DisplayPrefs } from "./types";

/** This PC's light mode, text size and on-screen keyboard (`local.json`); public, so the login
 * screen and the setup wizard follow them too. */
export function useDisplay() {
  return useQuery({ queryKey: qk.display, queryFn: () => commands.displayGet(), staleTime: Infinity });
}

/** Saved for this PC (`settings.hardware`) and applied to the whole window at once. */
export function useSetDisplay() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (prefs: DisplayPrefs) => commands.displaySet(prefs),
    onSuccess: (saved) => {
      client.setQueryData(qk.display, saved);
      applyDisplay(saved);
    },
  });
}

/** Mounted once in `App`: applies the saved choices at start and after each change. A failed read
 * keeps the defaults (normal text, full mode). */
export function useApplyDisplay(): void {
  const prefs = useDisplay().data;
  useEffect(() => {
    if (prefs) applyDisplay(prefs);
  }, [prefs]);
}
