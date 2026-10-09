import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { isLang, type Lang, setLanguage } from "@/lib/i18n";
import { applyTheme, isThemePreference, type ThemePreference } from "@/lib/theme";
import { commands } from "./client";
import { qk } from "./keys";
import type { JsonValue } from "./types";

export type SettingKey =
  | "ui.theme"
  | "ui.login_lang"
  | "security.auto_lock_minutes"
  | "shop.activity"
  | "numbering.restart_yearly"
  | "stamp_duty.mode"
  | "stock.negative_policy"
  | "sales.cash_rounding"
  | "cash.variance_threshold"
  | "cash.blind_close"
  | "sales.max_change"
  | "payments.cheque_requires_customer"
  | "stock.expired_lots"
  | "stock.expiry_alert_days"
  | "promotions.soldes_windows"
  | "products.internal_ean_prefix"
  | "company.nin"
  | "company.social"
  | "company.activity"
  | "company.ae_card"
  | "documents.layout"
  | "documents.texts"
  | "documents.quote_validity_days"
  | "documents.invoice_due_days"
  | "documents.arabic_guard"
  | "documents.arabic_guard_categories"
  | "scale.barcode_formats"
  | "labels.templates"
  | "labels.category_templates"
  | "labels.arabic_categories"
  | "cloud.sync.enabled"
  | "cloud.publish.mode"
  | "cloud.images.push"
  | "cloud.stock.push"
  | "cloud.stock.pull"
  | "cloud.prices.pull"
  | "cloud.sales.push"
  | "cloud.orders.intake"
  | "cloud.orders.document"
  | "cloud.orders.retention_days"
  | "cloud.customers.sync"
  | "cloud.customers.flags"
  | "cloud.receipt_qr"
  | "backup.keep_daily"
  | "backup.keep_weekly"
  | "backup.keep_monthly"
  | "backup.on_close"
  | "updates.channel"
  | "updates.auto_check"
  | "updates.auto_download"
  | "support.contact"
  | "packs.modules"
  | "packs.till_mode"
  | "sales.regulated_policy"
  | "wholesale.min_order_policy"
  | "documents.bl_as_invoice"
  | "restaurant.predefined_tickets"
  | "restaurant.kitchen_lang"
  | "restaurant.send_on_pay"
  | "restaurant.cover_product_id"
  | "flexy.operators"
  | "repairs.ready_message"
  | "repairs.conditions"
  | "repairs.uncollected_days"
  | "instalments.max_count"
  | "instalments.reminder_days"
  | "instalments.reminder_message"
  | "orders.layaway_min_deposit_bp"
  | "orders.kept_deposit_product_id"
  | "loyalty.points_per_100_da"
  | "loyalty.point_value"
  | "loyalty.min_redeem_points"
  | "loyalty.points_validity_months"
  | "vouchers.validity_months"
  | "jewellery.authorisation"
  | "jewellery.max_cash"
  | "payments.baridpay_qr"
  // M6 rows (core Task 17): the business-day cut-off, the hourly copies, the DPO
  | "sales.day_cutoff_hour"
  | "backup.keep_hourly"
  | "privacy.dpo";

export function useSetting(key: SettingKey, enabled = true) {
  return useQuery({ queryKey: qk.setting(key), queryFn: () => commands.settingsGet(key), enabled });
}

export function useSetSetting() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ key, value }: { key: SettingKey; value: JsonValue }) => commands.settingsSet(key, value),
    onSuccess: (value, { key }) => client.setQueryData(qk.setting(key), value),
  });
}

export function useThemePreference(enabled = true): ThemePreference {
  const { data } = useSetting("ui.theme", enabled);
  return isThemePreference(data) ? data : "system";
}

/** Applies at once, then persists. */
export function useSetTheme(): (preference: ThemePreference) => void {
  const { mutate } = useSetSetting();
  return useCallback(
    (preference: ThemePreference) => {
      applyTheme(preference);
      mutate({ key: "ui.theme", value: preference });
    },
    [mutate],
  );
}

export function useLoginLanguage(): Lang | undefined {
  const { data } = useSetting("ui.login_lang");
  return isLang(data) ? data : undefined;
}

export function useSetLoginLanguage(): (lang: Lang) => void {
  const { mutate } = useSetSetting();
  return useCallback(
    (lang: Lang) => {
      void setLanguage(lang);
      mutate({ key: "ui.login_lang", value: lang });
    },
    [mutate],
  );
}

export function useAutoLockMinutes(enabled: boolean): number {
  const { data } = useSetting("security.auto_lock_minutes", enabled);
  return typeof data === "number" ? data : 0;
}
