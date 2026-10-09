import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { commands } from "./client";
import { qk } from "./keys";
import type { AdvanceInput, Money, PaymentMethod, ProfileInput, ZakatInput } from "./types";

/** After an advance, a repayment or a deduction: the staff views, the drawer and the expenses. */
function useRefreshStaff() {
  const client = useQueryClient();
  return () =>
    Promise.all(
      [qk.staff, ["cash"], qk.expenses, ["reports"]].map((queryKey) =>
        client.invalidateQueries({ queryKey }),
      ),
    );
}

export function useStaff() {
  return useQuery({ queryKey: qk.staffList, queryFn: () => commands.staffList() });
}

export function useStaffEntries(userId: string | null) {
  return useQuery({
    queryKey: qk.staffEntries(userId ?? ""),
    queryFn: () => commands.staffEntries(userId ?? ""),
    enabled: Boolean(userId),
  });
}

export function useShifts(from: string, to: string, userId: string | null) {
  return useQuery({
    queryKey: qk.staffShifts(from, to, userId),
    queryFn: () => commands.staffShifts(from, to, userId),
  });
}

export function useSetProfile() {
  const refresh = useRefreshStaff();
  return useMutation({
    mutationFn: ({ userId, input }: { userId: string; input: ProfileInput }) =>
      commands.staffProfileSet(userId, input),
    onSuccess: refresh,
  });
}

export function useAdvance() {
  const refresh = useRefreshStaff();
  return useMutation({
    mutationFn: (input: AdvanceInput) => commands.staffAdvance(input),
    onSuccess: refresh,
  });
}

export function useRepay() {
  const refresh = useRefreshStaff();
  return useMutation({
    mutationFn: (v: { userId: string; amount: Money; method: PaymentMethod; note: string | null }) =>
      commands.staffRepay(v.userId, v.amount, v.method, v.note),
    onSuccess: refresh,
  });
}

/** `month` is the salary month, `YYYY-MM`. */
export function useDeduct() {
  const refresh = useRefreshStaff();
  return useMutation({
    mutationFn: (v: { userId: string; amount: Money; month: string; note: string | null }) =>
      commands.staffDeduct(v.userId, v.amount, v.month, v.note),
    onSuccess: refresh,
  });
}

/** The zakat figures as they would be saved now (every amount from the backend); idle for `null`. */
export function useZakatPreview(input: ZakatInput | null) {
  return useQuery({
    queryKey: qk.zakatPreview(input),
    queryFn: () => commands.zakatPreview(input as ZakatInput),
    enabled: input !== null,
    placeholderData: keepPreviousData,
    staleTime: 0,
  });
}

export function useZakatStatements() {
  return useQuery({ queryKey: qk.zakatList, queryFn: () => commands.zakatList() });
}

export function useSaveZakat() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (input: ZakatInput) => commands.zakatSave(input),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.zakat }),
  });
}

/** The saved statement on this PC's A4 printer, in `lang`. */
export function usePrintZakat() {
  return useMutation({
    mutationFn: ({ id, lang }: { id: string; lang: Lang }) => commands.zakatStatement(id, lang),
  });
}
