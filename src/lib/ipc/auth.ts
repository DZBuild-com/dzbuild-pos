import { type QueryClient, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { commands } from "./client";
import { PRIVATE_KEYS, qk } from "./keys";
import type { LoginInput, OverrideInput } from "./types";

export function resetPrivateData(client: QueryClient): void {
  for (const queryKey of PRIVATE_KEYS) client.removeQueries({ queryKey });
}

export function useSession(enabled = true) {
  return useQuery({ queryKey: qk.session, queryFn: () => commands.authSession(), enabled });
}

export function useUserTiles() {
  return useQuery({ queryKey: qk.tiles, queryFn: () => commands.authListUsers() });
}

export function useLogin() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (input: LoginInput) => commands.authLogin(input),
    onSuccess: (session) => {
      resetPrivateData(client);
      client.setQueryData(qk.session, session);
    },
  });
}

export function useLogout() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.authLogout(),
    onSettled: () => {
      resetPrivateData(client);
      client.setQueryData(qk.session, null);
      void client.invalidateQueries({ queryKey: qk.tiles });
    },
  });
}

export function useSetMyLanguage() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (lang: Lang) => commands.authSetLanguage(lang),
    onSuccess: (session) => client.setQueryData(qk.session, session),
  });
}

/** A manager's PIN for a protected action; the grant stays in Rust for the next operation. */
export function useManagerOverride() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (input: OverrideInput) => commands.authManagerOverride(input),
    onSuccess: () => client.invalidateQueries({ queryKey: ["sales", "preview"] }),
  });
}
