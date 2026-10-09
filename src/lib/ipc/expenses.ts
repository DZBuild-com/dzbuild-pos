import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { ExpenseCategoryInput, ExpenseQuery, NewExpense } from "./types";

export function useExpenseCategories(includeArchived = false) {
  return useQuery({
    queryKey: qk.expenseCategories(includeArchived),
    queryFn: () => commands.expensesCategories(includeArchived),
  });
}

function useRefreshCategories() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: qk.expenses });
}

/** Creates the category, or updates it when `id` is given. */
export function useSaveExpenseCategory() {
  const refresh = useRefreshCategories();
  return useMutation({
    mutationFn: ({ id, input }: { id?: string | null; input: ExpenseCategoryInput }) =>
      id ? commands.expensesCategoryUpdate(id, input) : commands.expensesCategoryCreate(input),
    onSuccess: refresh,
  });
}

export function useArchiveExpenseCategory() {
  const refresh = useRefreshCategories();
  return useMutation({
    mutationFn: (id: string) => commands.expensesCategoryArchive(id),
    onSuccess: refresh,
  });
}

/** An expense from the drawer moves the cash session; every expense moves the reports. */
function useRefreshAfterExpense() {
  const client = useQueryClient();
  return () =>
    Promise.all(
      [qk.expenses, ["cash"], ["reports"]].map((queryKey) => client.invalidateQueries({ queryKey })),
    );
}

export function useRecordExpense() {
  const refresh = useRefreshAfterExpense();
  return useMutation({
    mutationFn: (input: NewExpense) => commands.expensesRecord(input),
    onSuccess: refresh,
  });
}

/** A negative expense cancelling the original (never a deletion); `note` says why. */
export function useReverseExpense() {
  const refresh = useRefreshAfterExpense();
  return useMutation({
    mutationFn: ({ id, note }: { id: string; note: string }) => commands.expensesReverse(id, note),
    onSuccess: refresh,
  });
}

/** Expenses with the filters left out at their defaults (the latest ones). */
export function expenseQuery(filter: Partial<ExpenseQuery>): ExpenseQuery {
  return { from: null, to: null, categoryId: null, limit: null, ...filter };
}

export function useExpenses(filter: Partial<ExpenseQuery>, enabled = true) {
  const query = expenseQuery(filter);
  return useQuery({ queryKey: qk.expenseList(query), queryFn: () => commands.expensesList(query), enabled });
}

/** Net expenses per category between two business days (both included). */
export function useExpenseTotals(from: string, to: string, enabled = true) {
  return useQuery({
    queryKey: qk.expenseTotals(from, to),
    queryFn: () => commands.expensesTotals(from, to),
    enabled,
  });
}
