import { useQuery } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { Module, PackState } from "./types";

/** The shop's activity, the modules on and the till mode: every screen reads it. */
export function usePackState() {
  return useQuery({ queryKey: qk.packState, queryFn: () => commands.packsState(), staleTime: 60_000 });
}

/** True when the module is on (false while the state loads: pack screens stay hidden). */
export function useModule(module: Module): boolean {
  const { data } = usePackState();
  return data?.modules.includes(module) ?? false;
}

const NO_MODULES: Module[] = [];
const modulesOf = (state: PackState) => state.modules;

/** The modules on, for the till's parts (the sale screen, its payment and return dialogs) under
 * `TillEntry`, which reads the pack state first: mounting never refetches nor retries it, so a
 * failed read cannot make the till remount itself in a loop (`TillEntry` waits while it loads). */
export function usePackModules(): Module[] {
  const { data } = useQuery({
    queryKey: qk.packState,
    queryFn: () => commands.packsState(),
    staleTime: 60_000,
    refetchOnMount: false,
    retryOnMount: false,
    select: modulesOf,
  });
  return data ?? NO_MODULES;
}
