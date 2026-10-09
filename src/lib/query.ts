import { MutationCache, QueryCache, QueryClient } from "@tanstack/react-query";
import { toAppError } from "@/lib/ipc/client";
import { qk } from "@/lib/ipc/keys";

/** Local IPC never needs retries; an ended session anywhere returns to the login screen. Every call
 * goes to the local backend, so none waits for a network (`networkMode: "always"`): by default
 * TanStack pauses them all when the webview reports `offline` (a cable or Wi-Fi drops), and the
 * till would stop selling. */
export function createQueryClient(): QueryClient {
  const client: QueryClient = new QueryClient({
    queryCache: new QueryCache({
      onError: (error) => {
        const code = toAppError(error).code;
        if (code === "session_required") client.setQueryData(qk.session, null);
        // M9: a write refused for the licence means it changed (a trial ended): read it again.
        if (code.startsWith("license_")) void client.invalidateQueries({ queryKey: qk.license });
      },
    }),
    mutationCache: new MutationCache({
      onError: (error) => {
        const code = toAppError(error).code;
        if (code === "session_required") client.setQueryData(qk.session, null);
        // M9: a write refused for the licence means it changed (a trial ended): read it again.
        if (code.startsWith("license_")) void client.invalidateQueries({ queryKey: qk.license });
      },
    }),
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: false, staleTime: 30_000, networkMode: "always" },
      mutations: { retry: false, networkMode: "always" },
    },
  });
  return client;
}
