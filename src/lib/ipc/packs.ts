import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { DepartmentKeyInput, Module, ShopActivity, TillMode } from "./types";

// The pack state is read by the shell's rail (the till's entry): it lives in its own small module
// so the rest of these hooks stay in the pack screens' chunks.
export { useModule, usePackState } from "./pack-state";

/** Every activity template (the setup wizard lists them before the owner exists). */
export function useTemplates() {
  return useQuery({
    queryKey: qk.packTemplates,
    queryFn: () => commands.packsTemplates(),
    staleTime: Infinity,
  });
}

/** A module, template or till-mode change: every screen reads the pack state, so every query goes. */
function useRefreshPacks() {
  const client = useQueryClient();
  return () => client.invalidateQueries();
}

export function useApplyTemplate() {
  const refresh = useRefreshPacks();
  return useMutation({
    mutationFn: ({ activity, seed }: { activity: ShopActivity; seed: boolean }) =>
      commands.packsApplyTemplate(activity, seed),
    onSuccess: refresh,
  });
}

export function useSetModules() {
  const refresh = useRefreshPacks();
  return useMutation({
    mutationFn: (modules: Module[]) => commands.packsSetModules(modules),
    onSuccess: refresh,
  });
}

/** Back to the template's modules. */
export function useResetModules() {
  const refresh = useRefreshPacks();
  return useMutation({ mutationFn: () => commands.packsResetModules(), onSuccess: refresh });
}

export function useSetTillMode() {
  const refresh = useRefreshPacks();
  return useMutation({ mutationFn: (mode: TillMode) => commands.packsSetTillMode(mode), onSuccess: refresh });
}

/** The regulated staples at `levelId` (the retail level when `null`) against their ceiling. */
export function useRegulatedBoard(levelId: string | null = null, enabled = true) {
  return useQuery({
    queryKey: qk.regulated(levelId),
    queryFn: () => commands.regulatedBoard(levelId),
    enabled,
  });
}

/** Links a product to a ceiling of the data pack (`code: null` removes the link). */
export function useSetRegulated() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ productId, code }: { productId: string; code: string | null }) =>
      commands.regulatedSet(productId, code),
    onSuccess: () => client.invalidateQueries({ queryKey: ["packs", "regulated"] }),
  });
}

/** Creates the data pack's regulated staples not in the catalogue yet; resolves with how many. */
export function useSeedRegulated() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.regulatedSeed(),
    onSuccess: () =>
      Promise.all([
        client.invalidateQueries({ queryKey: ["packs", "regulated"] }),
        client.invalidateQueries({ queryKey: ["products"] }),
      ]),
  });
}

/** The department keys of the till's calculator. */
export function useDepartments(includeArchived = false, enabled = true) {
  return useQuery({
    queryKey: qk.departments(includeArchived),
    queryFn: () => commands.departmentsList(includeArchived),
    enabled,
  });
}

function useRefreshDepartments() {
  const client = useQueryClient();
  return () =>
    Promise.all([
      client.invalidateQueries({ queryKey: ["packs", "departments"] }),
      client.invalidateQueries({ queryKey: ["products"] }),
    ]);
}

/** Creates a key (`id: null`) or changes one; its product follows. */
export function useSaveDepartment() {
  const refresh = useRefreshDepartments();
  return useMutation({
    mutationFn: ({ id, input }: { id: string | null; input: DepartmentKeyInput }) =>
      commands.departmentsSave(id, input),
    onSuccess: refresh,
  });
}

export function useArchiveDepartment() {
  const refresh = useRefreshDepartments();
  return useMutation({ mutationFn: (id: string) => commands.departmentsArchive(id), onSuccess: refresh });
}
