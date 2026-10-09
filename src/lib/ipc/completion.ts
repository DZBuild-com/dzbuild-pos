import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, toAppError } from "./client";
import { qk } from "./keys";
import type { PaymentType, PaymentTypeInput, RoleNames } from "./types";

/** The custom payment types (the till's tiles and the settings list). */
export function usePaymentTypes(includeArchived = false) {
  return useQuery({
    queryKey: qk.paymentTypes(includeArchived),
    queryFn: () => commands.paymentTypesList(includeArchived),
  });
}

const NO_TYPES: PaymentType[] = [];

/** The live custom payment types the till's payment dialog shows after the base tenders (core
 * Task 16); none while they are not answered, nor for a table ticket (`enabled` false). */
export function useTillPaymentTypes(enabled: boolean): PaymentType[] {
  const { data } = useQuery({
    queryKey: qk.paymentTypes(false),
    queryFn: () => commands.paymentTypesList(false),
    enabled,
    staleTime: 60_000,
  });
  return data ?? NO_TYPES;
}

function useRefreshPaymentTypes() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: ["paymentTypes"] });
}

/** Creates the type when `id` is `null`, else changes it. */
export function useSavePaymentType() {
  const refresh = useRefreshPaymentTypes();
  return useMutation({
    mutationFn: ({ id, input }: { id: string | null; input: PaymentTypeInput }) =>
      commands.paymentTypeSave(id, input),
    onSuccess: refresh,
  });
}

export function useArchivePaymentType() {
  const refresh = useRefreshPaymentTypes();
  return useMutation({ mutationFn: (id: string) => commands.paymentTypeArchive(id), onSuccess: refresh });
}

function useRefreshPeople() {
  const client = useQueryClient();
  return () =>
    Promise.all(
      [qk.users, qk.roles, qk.tiles, qk.session, qk.staff].map((queryKey) =>
        client.invalidateQueries({ queryKey }),
      ),
    );
}

/** A custom role with the permissions of another (the owner's). */
export function useDuplicateRole() {
  const refresh = useRefreshPeople();
  return useMutation({
    mutationFn: ({ roleId, names }: { roleId: string; names: RoleNames }) =>
      commands.rolesDuplicate(roleId, names),
    onSuccess: refresh,
  });
}

/** A new user on the model of another, with their own name and PIN. */
export function useDuplicateUser() {
  const refresh = useRefreshPeople();
  return useMutation({
    mutationFn: ({
      templateId,
      displayName,
      pin,
    }: {
      templateId: string;
      displayName: string;
      pin: string;
    }) => commands.usersDuplicate(templateId, displayName, pin),
    onSuccess: refresh,
  });
}

/** The variants of a model as a grid with their stock; idle without a model. */
export function useVariantGrid(parentId: string | null, locationId: string | null, rowAxis: string | null) {
  return useQuery({
    queryKey: qk.variantGrid(parentId ?? "", locationId, rowAxis),
    queryFn: () => commands.variantGrid(parentId ?? "", locationId, rowAxis),
    enabled: Boolean(parentId),
  });
}

/** VRT-060: a model's stock grid at `locationId` (every location when `null`), its rows on
 * `rowAxis` (the first axis when `null`); an axis the model no longer has (kept on this PC) falls
 * back to the first one. Stock data: a stock or catalogue change refreshes it. */
export function useVariantGridOf(parentId: string, locationId: string | null, rowAxis: string | null) {
  return useQuery({
    queryKey: ["stock", "variantGrid", parentId, locationId, rowAxis],
    queryFn: async () => {
      try {
        return await commands.variantGrid(parentId, locationId, rowAxis);
      } catch (error) {
        if (rowAxis === null || toAppError(error).field !== "rowAxis") throw error;
        return commands.variantGrid(parentId, locationId, null);
      }
    },
    placeholderData: keepPreviousData,
  });
}

/** Saves the text of the shop's BaridPay QR (`payments.baridpay_qr`, `null` removes it); the QR
 * the customer screen shows is read again. */
export function useSetBaridpayQr() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (text: string | null) => commands.settingsSet("payments.baridpay_qr", text),
    onSuccess: (value) =>
      Promise.all([
        client.setQueryData(qk.setting("payments.baridpay_qr"), value),
        client.invalidateQueries({ queryKey: qk.baridpayQr }),
      ]),
  });
}

/** The shop's BaridiMob QR for the customer screen (`null` until the owner sets it). */
export function useBaridpayQr() {
  return useQuery({
    queryKey: qk.baridpayQr,
    queryFn: () => commands.paymentsBaridpayQr(),
    staleTime: 60_000,
  });
}
