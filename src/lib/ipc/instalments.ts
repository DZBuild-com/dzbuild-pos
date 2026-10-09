import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { commands } from "./client";
import { qk } from "./keys";
import type { GuaranteeInput, PlanFilter, PlanPayment, QuoteInput, SignInput } from "./types";

/** Invalidates every query under these keys once a write succeeded. */
function useRefresh(keys: ReadonlyArray<readonly unknown[]>) {
  const client = useQueryClient();
  return () => Promise.all(keys.map((queryKey) => client.invalidateQueries({ queryKey })));
}

/** After a contract or a versement: the plans, the customer's kridi, the drawer and the reports. */
const ACCOUNT_KEYS: ReadonlyArray<readonly unknown[]> = [
  qk.instalments,
  qk.customers,
  ["credit"],
  ["cash"],
  ["reports"],
];

/** The contract's figures while the cashier types: the price, the duty on the down payment's cash,
 * what to receive now and the schedule all come from the backend. */
export function useInstalmentQuote(input: QuoteInput | null) {
  return useQuery({
    queryKey: qk.instalmentQuote(input),
    queryFn: () => commands.instalmentsQuote(input as QuoteInput),
    enabled: input !== null,
    placeholderData: input === null ? undefined : keepPreviousData,
    staleTime: 0,
  });
}

/** Signs the contract: its ticket is sold (the receipt queued), the rest goes on the account. */
export function useSignInstalment() {
  const refresh = useRefresh([...ACCOUNT_KEYS, ["sales"], ["stock"]]);
  return useMutation({
    mutationFn: ({ input, printReceipt }: { input: SignInput; printReceipt: boolean }) =>
      commands.instalmentsSign(input, printReceipt),
    onSuccess: refresh,
  });
}

/** The contract's page for the in-app preview; `null` asks nothing. */
export function useContractPage(planId: string | null, lang: Lang) {
  return useQuery({
    queryKey: qk.contract(planId ?? "", lang),
    queryFn: () => commands.instalmentsContract(planId ?? "", lang),
    enabled: planId !== null,
  });
}

/** Prints the contract on this PC's A4 printer through the print host. */
export function usePrintContract() {
  return useMutation({
    mutationFn: ({ planId, lang }: { planId: string; lang: Lang }) =>
      commands.instalmentsContractPrint(planId, lang),
  });
}

export function usePlan(planId: string | null) {
  return useQuery({
    queryKey: qk.plan(planId ?? ""),
    queryFn: () => commands.instalmentsGet(planId ?? ""),
    enabled: planId !== null,
  });
}

export function usePlans(filter: PlanFilter) {
  return useQuery({
    queryKey: qk.plans(filter),
    queryFn: () => commands.instalmentsList(filter),
    placeholderData: keepPreviousData,
  });
}

/** A versement on a contract: its `RV…` receipt prints (and the drawer opens for its cash). */
export function useCollectInstalment() {
  const refresh = useRefresh(ACCOUNT_KEYS);
  return useMutation({
    mutationFn: ({
      planId,
      payment,
      printReceipt,
    }: {
      planId: string;
      payment: PlanPayment;
      printReceipt: boolean;
    }) => commands.instalmentsCollect(planId, payment, printReceipt),
    onSuccess: refresh,
  });
}

/** Dues coming within `withinDays` (`null`: the `instalments.reminder_days` setting) and late ones. */
export function useReminders(withinDays: number | null = null) {
  return useQuery({
    queryKey: qk.reminders(withinDays),
    queryFn: () => commands.instalmentsReminders(withinDays),
  });
}

/** The reminder of a due to copy into an SMS or WhatsApp, in `lang`. */
export function useReminderText(planId: string | null, seq: number, lang: Lang) {
  return useQuery({
    queryKey: qk.reminderText(planId ?? "", seq, lang),
    queryFn: () => commands.instalmentsReminderText(planId ?? "", seq, lang),
    enabled: planId !== null,
  });
}

export function useNoteReminder() {
  const refresh = useRefresh([qk.instalments]);
  return useMutation({
    mutationFn: ({ planId, seq, note = null }: { planId: string; seq: number; note?: string | null }) =>
      commands.instalmentsNoteReminder(planId, seq, note),
    onSuccess: refresh,
  });
}

/** Adds a supporting document to a signed contract (never a cheque: the kinds are a closed list). */
export function useAddGuarantee() {
  const refresh = useRefresh([qk.instalments]);
  return useMutation({
    mutationFn: ({ planId, guarantee }: { planId: string; guarantee: GuaranteeInput }) =>
      commands.instalmentsAddGuarantee(planId, guarantee),
    onSuccess: refresh,
  });
}
