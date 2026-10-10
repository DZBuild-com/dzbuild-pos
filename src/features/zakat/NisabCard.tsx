import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formatAmountInput } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { qk } from "@/lib/ipc/keys";
import { useTillLink } from "@/lib/ipc/lan";
import { type SettingKey, useSetSetting, useSetting } from "@/lib/ipc/settings";
import type { JsonValue } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** The longest source note `zakat.nisab_source` takes (pos-core's validator). */
const SOURCE_MAX = 120;

/**
 * The nisab of the current Hijri year, as the shop enters it: the figure the Ministry of
 * Religious Affairs publishes, typed by the owner, with a short note on where it comes from. The
 * software never fills it in, and computes nothing until it is entered. Editable by a holder of
 * `settings.company` on a single PC or the main PC; shown read-only otherwise (a till mirrors the
 * main PC's settings).
 */
export function NisabCard() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const client = useQueryClient();
  const session = useSession();
  const till = useTillLink();
  const save = useSetSetting();
  const storedNisab = useSetting("zakat.nisab").data;
  const storedSource = useSetting("zakat.nisab_source").data;
  const nisab = typeof storedNisab === "number" && storedNisab > 0 ? storedNisab : null;
  const source = typeof storedSource === "string" ? storedSource : "";
  // Only what the user typed is state: the settings may arrive after the card mounts, and an
  // untouched field shows the stored value and is not written again.
  const [amount, setAmount] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [badAmount, setBadAmount] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const manages = session.data?.permissions.includes("settings.company") ?? false;
  const readOnly = !manages || till.isTill;
  const amountText = amount ?? (nisab === null ? "" : formatAmountInput(nisab, lang));
  const noteText = note ?? source;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setError(null);
    const value = parseAmount(amountText);
    if (value === null || value <= 0) {
      setBadAmount(true);
      return;
    }
    const writes: Array<{ key: SettingKey; value: JsonValue }> = [];
    if (value !== nisab) writes.push({ key: "zakat.nisab", value });
    if (noteText.trim() !== source) writes.push({ key: "zakat.nisab_source", value: noteText.trim() });
    try {
      for (const write of writes) await save.mutateAsync(write);
      setAmount(null);
      setNote(null);
      // the calculation is redone against the new figure
      await client.invalidateQueries({ queryKey: qk.zakat });
      toast.success(t("zakat:nisabCard.saved"));
    } catch (e) {
      setError(e);
    }
  };

  return (
    <Card aria-labelledby="zakat-nisab-title">
      <h2 id="zakat-nisab-title" className="text-xl font-bold">
        {t("zakat:nisabCard.title")}
      </h2>
      <p className="text-ink-2">{t("zakat:nisabCard.help")}</p>
      {till.isTill ? (
        <p role="note" className="rounded-control bg-info-soft p-3 text-info">
          {t("zakat:nisabCard.mainPcOnly")}
        </p>
      ) : !manages ? (
        <p role="note" className="rounded-control bg-info-soft p-3 text-info">
          {t("zakat:nisabCard.ownerOnly")}
        </p>
      ) : null}
      <form noValidate onSubmit={(event) => void submit(event)} className="flex flex-col gap-4">
        <fieldset disabled={readOnly} className="grid gap-4 sm:grid-cols-2">
          <Field
            id="zakat-nisab"
            label={t("zakat:nisabCard.amount")}
            hint={nisab === null ? t("zakat:nisabCard.notSet") : undefined}
            error={badAmount ? t("validation:amountFormat") : undefined}
          >
            <NumericInput
              id="zakat-nisab"
              aria-invalid={badAmount}
              value={amountText}
              onValueChange={(v) => {
                setAmount(v);
                setBadAmount(false);
              }}
            />
          </Field>
          <Field
            id="zakat-nisab-source"
            label={t("zakat:nisabCard.source")}
            hint={t("zakat:nisabCard.sourceHint")}
            optional={t("customers:form.optional")}
          >
            <Input
              id="zakat-nisab-source"
              dir="auto"
              maxLength={SOURCE_MAX}
              value={noteText}
              onChange={(e) => setNote(e.target.value)}
            />
          </Field>
        </fieldset>
        {error ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
        {readOnly ? null : (
          <Button type="submit" variant="primary" className="self-start" disabled={save.isPending}>
            {t("zakat:nisabCard.save")}
          </Button>
        )}
      </form>
    </Card>
  );
}
