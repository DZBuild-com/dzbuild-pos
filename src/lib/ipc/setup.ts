import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, toAppError } from "./client";
import { qk } from "./keys";
import type { FirstRunInput, SetupStatus } from "./types";

export function useSetupStatus(enabled: boolean) {
  return useQuery({ queryKey: qk.setup, queryFn: () => commands.setupStatus(), enabled });
}

/** The wizard's end (printer step done or skipped): the gate may now open the app. Also used
 * when another start already finished the setup. No refetch: the backend already says so. */
export function useFinishSetup(): () => void {
  const client = useQueryClient();
  return () =>
    client.setQueryData<SetupStatus>(qk.setup, (old) =>
      old ? { ...old, completed: true, hasOwner: true } : old,
    );
}

export function useCompleteFirstRun() {
  const client = useQueryClient();
  const finish = useFinishSetup();
  return useMutation({
    mutationFn: (input: FirstRunInput) => commands.setupCompleteFirstRun(input),
    // The shop exists and the owner is signed in; the gate keeps showing the wizard (cached
    // `completed: false`) until the printer step ends.
    onSuccess: (session) => client.setQueryData(qk.session, session),
    // Another start already finished the setup: the app gate moves on to the login screen
    // instead of leaving the wizard on an error it cannot fix.
    onError: (error) => {
      if (toAppError(error).code === "setup_already_done") finish();
    },
  });
}
