import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { commands, events } from "./client";
import { qk } from "./keys";
import type { ExternalLink, FolderKind } from "./types";

export function useStartupError() {
  return useQuery({ queryKey: qk.startup, queryFn: () => commands.appStartupError(), staleTime: Infinity });
}

/** Notices of this start (a failed restore, an outdated WebView2, a clock set back, an update);
 * fetched once the database opened. */
export function useStartupNotices(enabled: boolean) {
  return useQuery({
    queryKey: qk.notices,
    queryFn: () => commands.appStartupNotices(),
    staleTime: Infinity,
    enabled,
  });
}

export function useAppInfo() {
  return useQuery({ queryKey: qk.appInfo, queryFn: () => commands.appInfo() });
}

/** The app data folder, for `mediaUrl` (product photos); it never changes while the app runs. */
export function useAppDataDir() {
  return useQuery({ queryKey: qk.dataDir, queryFn: () => commands.appDataDir(), staleTime: Infinity });
}

export function useDatapack() {
  return useQuery({ queryKey: qk.datapack, queryFn: () => commands.datapackGet(), staleTime: Infinity });
}

export function useOpenFolder() {
  return useMutation({ mutationFn: (kind: FolderKind) => commands.appOpenFolder(kind) });
}

/** NFR-003: marks the first usable screen once (the backend measures from the process start). A
 * failed mark only loses the measure, never the screen. */
export function useAppReady(ready: boolean): void {
  const sent = useRef(false);
  useEffect(() => {
    if (!ready || sent.current) return;
    sent.current = true;
    void commands.appReady().catch(() => undefined);
  }, [ready]);
}

/** Opens one of the fixed links (the WebView2 download, the DZBuild pages) in the browser: Rust
 * names the URL, the webview only its kind. */
export function useOpenLink() {
  return useMutation({ mutationFn: (kind: ExternalLink) => commands.appOpenLink(kind) });
}

/** DAT-024: `true` once the app said it backs up before closing (`app-closing`); it stays so
 * until the process ends. */
export function useAppClosing(): boolean {
  const [closing, setClosing] = useState(false);
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    void events.appClosing
      .listen((event) => setClosing(event.payload.backup))
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      // Without the listener the app still backs up and closes; only the message is lost.
      .catch((error: unknown) => console.error("app-closing listener refused:", error));
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);
  return closing;
}
