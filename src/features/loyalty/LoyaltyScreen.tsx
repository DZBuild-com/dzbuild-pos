import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { formatAmountInput } from "@/lib/format";
import { useTillLink } from "@/lib/ipc/lan";
import { useModule } from "@/lib/ipc/packs";
import { type SettingKey, useSetSetting, useSetting } from "@/lib/ipc/settings";
import type { JsonValue, Permission, Session } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { VouchersList } from "./VouchersList";

type Tab = "vouchers" | "loyalty";

/** `/loyalty` (LOY-001, LOY-002 lite): the vouchers and what the shop owes their holders
 * (`reports.cash`), and the loyalty rules (read by everyone here, changed with
 * `settings.company`). */
export function LoyaltyScreen({ session }: { session: Session }) {
  const { t } = useTranslation();
  const [params, setParams] = useSearchParams();
  const vouchersOn = useModule("gift_vouchers");
  const loyaltyOn = useModule("loyalty");
  const can = (p: Permission) => session.permissions.includes(p);
  const tabs: Tab[] = [
    ...(vouchersOn && can("reports.cash") ? (["vouchers"] as const) : []),
    ...(loyaltyOn ? (["loyalty"] as const) : []),
  ];
  const requested = params.get("tab");
  const tab = tabs.find((k) => k === requested) ?? tabs[0];
  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-3xl font-bold">{t("loyalty:title")}</h1>
      <div
        role="tablist"
        aria-label={t("loyalty:tabs.label")}
        className="flex flex-wrap gap-2 border-b border-line pb-2"
      >
        {tabs.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`loyalty-tab-${k}`}
            aria-selected={tab === k}
            aria-controls={`loyalty-panel-${k}`}
            onClick={() => setParams({ tab: k }, { replace: true })}
            className={cn(
              "min-h-11 rounded-control px-4 font-semibold",
              tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
            )}
          >
            {t(`loyalty:tabs.${k}`)}
          </button>
        ))}
      </div>
      {tab ? (
        <div role="tabpanel" id={`loyalty-panel-${tab}`} aria-labelledby={`loyalty-tab-${tab}`}>
          {tab === "vouchers" ? (
            <VouchersList canReprint={can("sales.reprint")} />
          ) : (
            <LoyaltySettings editable={can("settings.company")} />
          )}
        </div>
      ) : null}
    </div>
  );
}

interface Rule {
  key: SettingKey;
  label: "pointsPer100" | "pointValue" | "minRedeem" | "validity" | "voucherValidity";
  /** `amount`: centimes typed as DA; `integer`: a count. */
  kind: "amount" | "integer";
}

/** The M0 settings of the programme (pos-core `settings::REGISTRY` checks each range). */
const RULES: readonly Rule[] = [
  { key: "loyalty.points_per_100_da", label: "pointsPer100", kind: "integer" },
  { key: "loyalty.point_value", label: "pointValue", kind: "amount" },
  { key: "loyalty.min_redeem_points", label: "minRedeem", kind: "integer" },
  { key: "loyalty.points_validity_months", label: "validity", kind: "integer" },
  { key: "vouchers.validity_months", label: "voucherValidity", kind: "integer" },
];

/** The rules of the loyalty programme; the main PC's on a till (read-only there). */
function LoyaltySettings({ editable }: { editable: boolean }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const save = useSetSetting();
  const till = useTillLink();
  const readOnly = !editable || till.isTill;
  const stored = [
    useSetting("loyalty.points_per_100_da").data,
    useSetting("loyalty.point_value").data,
    useSetting("loyalty.min_redeem_points").data,
    useSetting("loyalty.points_validity_months").data,
    useSetting("vouchers.validity_months").data,
  ];
  const [typed, setTyped] = useState<Partial<Record<SettingKey, string>>>({});
  const [invalid, setInvalid] = useState<SettingKey | null>(null);
  const [error, setError] = useState<unknown>(null);
  const lang = useLang();
  const shown = (rule: Rule, value: JsonValue | undefined) =>
    typed[rule.key] ??
    (typeof value !== "number"
      ? ""
      : rule.kind === "amount"
        ? formatAmountInput(value, lang)
        : String(value));
  const parse = (rule: Rule, text: string): number | null => {
    if (rule.kind === "amount") return parseAmount(text);
    return /^\d+$/.test(text.trim()) ? Number(text.trim()) : null;
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setError(null);
    const values: Array<[SettingKey, number]> = [];
    for (const [i, rule] of RULES.entries()) {
      const text = typed[rule.key];
      if (text === undefined) continue;
      const value = parse(rule, text);
      if (value === null) {
        setInvalid(rule.key);
        return;
      }
      if (value !== stored[i]) values.push([rule.key, value]);
    }
    try {
      for (const [key, value] of values) await save.mutateAsync({ key, value });
      setTyped({});
      toast.success(t("loyalty:settings.saved"));
    } catch (e) {
      setError(e);
    }
  };
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("loyalty:settings.title")}</h2>
      <HostOnlyNote />
      <form noValidate onSubmit={(event) => void submit(event)} className="flex flex-col gap-4">
        <fieldset disabled={readOnly} className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {RULES.map((rule, i) => (
            <Field
              key={rule.key}
              id={`loyalty-rule-${rule.label}`}
              label={t(`loyalty:settings.${rule.label}`)}
              hint={rule.label === "validity" ? t("loyalty:settings.validityHint") : undefined}
              error={
                invalid === rule.key
                  ? t(rule.kind === "amount" ? "validation:amountFormat" : "validation:required")
                  : undefined
              }
            >
              <NumericInput
                id={`loyalty-rule-${rule.label}`}
                kind={rule.kind}
                value={shown(rule, stored[i])}
                onValueChange={(value) => {
                  setTyped((current) => ({ ...current, [rule.key]: value }));
                  setInvalid(null);
                }}
              />
            </Field>
          ))}
        </fieldset>
        {error ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
        {readOnly ? null : (
          <Button type="submit" variant="primary" className="self-start" disabled={save.isPending}>
            {t("loyalty:settings.save")}
          </Button>
        )}
      </form>
    </Card>
  );
}
