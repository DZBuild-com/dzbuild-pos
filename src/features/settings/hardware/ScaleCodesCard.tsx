import { Plus } from "lucide-react";
import { type KeyboardEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { ScanInput } from "@/components/scan-input";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { formatDZD } from "@/lib/format";
import { useDatapack } from "@/lib/ipc/app";
import { toAppError } from "@/lib/ipc/client";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { useTillLink } from "@/lib/ipc/lan";
import { useFindByCode } from "@/lib/ipc/products";
import { useSetSetting, useSetting } from "@/lib/ipc/settings";
import type { ScaleBarcodeLayout, ScaleValue } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const VALUES: readonly ScaleValue[] = ["weight", "price", "count"];
const DIGITS = [4, 5, 6];
const DECIMALS = [0, 1, 2, 3];
/** pos-core `scale_codes::MAX_LAYOUTS`. */
const MAX_LAYOUTS = 10;

/** A weight label of the most common layout: `21` PLU×5 weight×5 in grams. */
const NEW_LAYOUT: ScaleBarcodeLayout = {
  prefix: "",
  pluDigits: 5,
  verifier: false,
  value: "weight",
  valueDigits: 5,
  decimals: 3,
};

/** "Codes-barres poids et prix" (HW-030): the shop's layouts, else the regulatory pack's. A shop
 * setting (`scale.barcode_formats`), saved on its own and not in `hardware.json`; decoded in Rust
 * only (`products_find_by_code`). */
export function ScaleCodesCard() {
  const formats = useSetting("scale.barcode_formats");
  const internal = useSetting("products.internal_ean_prefix");
  const pack = useDatapack();
  // here, not in the editor: a save that changes the layouts starts a new editor, and its outcome
  // stays in view
  const save = useSetSetting();
  if (formats.isPending || pack.isPending) return null;
  const custom = Array.isArray(formats.data) ? (formats.data as unknown as ScaleBarcodeLayout[]) : null;
  const inEffect = custom ?? pack.data?.scaleBarcodes?.defaultLayouts ?? [];
  return (
    <CodesEditor
      // a save or a reset starts the editor again from what is now in effect
      key={JSON.stringify(inEffect)}
      save={save}
      initial={inEffect}
      fromPack={custom === null}
      internal={typeof internal.data === "string" ? internal.data : ""}
    />
  );
}

function CodesEditor({
  save,
  initial,
  fromPack,
  internal,
}: {
  save: ReturnType<typeof useSetSetting>;
  initial: ScaleBarcodeLayout[];
  fromPack: boolean;
  internal: string;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const [rows, setRows] = useState<ScaleBarcodeLayout[]>(initial);
  // The layouts are a shop setting of the main PC; on a till they are read-only (the
  // label test below still works).
  const readOnly = useTillLink().isTill;
  const failed = save.error ? toAppError(save.error) : null;
  const errorAt = (i: number, field: keyof ScaleBarcodeLayout) =>
    failed?.field === `layouts.${i}.${field}` ? errorText(failed) : undefined;
  const rowError = failed?.field?.startsWith("layouts.") ? null : failed;
  const setRow = (i: number, change: Partial<ScaleBarcodeLayout>) =>
    setRows(rows.map((row, j) => (j === i ? { ...row, ...change } : row)));
  const overlaps = [...new Set(rows.map((r) => r.prefix))].filter(
    (prefix) => prefix.length > 0 && internal.startsWith(prefix),
  );

  return (
    <Card>
      <h2 className="text-xl font-bold">{t("hardware:scale.codes.title")}</h2>
      <HostOnlyNote />
      {fromPack ? <p className="text-ink-2">{t("hardware:scale.codes.fromPack")}</p> : null}
      {overlaps.map((prefix) => (
        <p key={prefix} className="rounded-card bg-warn-soft p-3 text-warn">
          {t("hardware:scale.codes.overlap", { prefix, internal })}
        </p>
      ))}
      <fieldset disabled={readOnly} className="contents">
        <ul className="flex flex-col gap-3">
          {rows.map((row, i) => (
            // rows have no id: a layout is its content (the backend names a row by its index too)
            <li key={i} className="grid gap-3 rounded-card border border-line p-3 sm:grid-cols-3">
              <Field
                id={`layout-${i}-prefix`}
                label={t("hardware:scale.codes.prefix")}
                error={errorAt(i, "prefix")}
              >
                <Input
                  id={`layout-${i}-prefix`}
                  dir="ltr"
                  inputMode="numeric"
                  maxLength={2}
                  value={row.prefix}
                  onChange={(e) => setRow(i, { prefix: e.target.value.replace(/\D/g, "") })}
                />
              </Field>
              <Field
                id={`layout-${i}-plu`}
                label={t("hardware:scale.codes.pluDigits")}
                error={errorAt(i, "pluDigits")}
              >
                <NativeSelect
                  id={`layout-${i}-plu`}
                  dir="ltr"
                  value={row.pluDigits}
                  onChange={(e) => setRow(i, { pluDigits: Number(e.target.value) })}
                >
                  {DIGITS.map((d) => (
                    <option key={d} value={d}>
                      {d}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
              <Field
                id={`layout-${i}-value`}
                label={t("hardware:scale.codes.value")}
                error={errorAt(i, "value")}
              >
                <NativeSelect
                  id={`layout-${i}-value`}
                  value={row.value}
                  onChange={(e) => setRow(i, { value: e.target.value as ScaleValue })}
                >
                  {VALUES.map((v) => (
                    <option key={v} value={v}>
                      {t(`hardware:scale.codes.values.${v}`)}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
              <Field
                id={`layout-${i}-digits`}
                label={t("hardware:scale.codes.valueDigits")}
                error={errorAt(i, "valueDigits")}
              >
                <NativeSelect
                  id={`layout-${i}-digits`}
                  dir="ltr"
                  value={row.valueDigits}
                  onChange={(e) => setRow(i, { valueDigits: Number(e.target.value) })}
                >
                  {DIGITS.map((d) => (
                    <option key={d} value={d}>
                      {d}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
              <Field
                id={`layout-${i}-decimals`}
                label={t("hardware:scale.codes.decimals")}
                error={errorAt(i, "decimals")}
              >
                <NativeSelect
                  id={`layout-${i}-decimals`}
                  dir="ltr"
                  value={row.decimals}
                  onChange={(e) => setRow(i, { decimals: Number(e.target.value) })}
                >
                  {DECIMALS.map((d) => (
                    <option key={d} value={d}>
                      {d}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
              <div className="flex flex-wrap items-end justify-between gap-2">
                <label className="flex min-h-11 items-center gap-3 font-semibold">
                  <Checkbox
                    checked={row.verifier}
                    onCheckedChange={(v) => setRow(i, { verifier: v === true })}
                  />
                  {t("hardware:scale.codes.verifier")}
                </label>
                {readOnly ? null : (
                  <Button variant="ghost" onClick={() => setRows(rows.filter((_, j) => j !== i))}>
                    {t("common:actions.remove")}
                  </Button>
                )}
              </div>
            </li>
          ))}
        </ul>
      </fieldset>
      {readOnly ? null : (
        <div className="flex flex-wrap gap-2">
          <Button
            variant="secondary"
            disabled={rows.length >= MAX_LAYOUTS}
            onClick={() => setRows([...rows, NEW_LAYOUT])}
          >
            <Plus aria-hidden />
            {t("hardware:scale.codes.add")}
          </Button>
          <Button
            variant="outline"
            disabled={save.isPending}
            onClick={() => save.mutate({ key: "scale.barcode_formats", value: rows })}
          >
            {t("hardware:scale.codes.save")}
          </Button>
          {fromPack ? null : (
            <Button
              variant="ghost"
              disabled={save.isPending}
              onClick={() => save.mutate({ key: "scale.barcode_formats", value: null })}
            >
              {t("hardware:scale.codes.reset")}
            </Button>
          )}
        </div>
      )}
      {save.isSuccess ? <p className="text-ok">{t("hardware:scale.codes.saved")}</p> : null}
      {rowError ? (
        <p role="alert" className="text-danger">
          {errorText(rowError)}
        </p>
      ) : null}
      <CodeTest />
    </Card>
  );
}

/** "Scannez une étiquette de balance": what the till will make of a label. */
function CodeTest() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const find = useFindByCode();
  const [code, setCode] = useState("");
  const hit = find.data;
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if ((event.key === "Enter" || event.key === "Tab") && code.trim() !== "") {
      event.preventDefault();
      find.mutate(code.trim());
      setCode("");
    }
  };
  const name = hit ? localizedName(hit.product, lang) : "";
  const qty = hit ? formatQty(hit.packQty, lang) : "";
  return (
    <div className="flex flex-col gap-2 border-t border-line pt-4">
      <Field id="scale-code-test" label={t("hardware:scale.codes.test")}>
        {/* a scanned label is read with the scanner's layout (an AZERTY Windows garbles it
            otherwise); a code typed by hand ends with Enter or Tab */}
        <ScanInput
          id="scale-code-test"
          dir="ltr"
          value={code}
          onChange={(e) => setCode(e.target.value)}
          onKeyDown={onKeyDown}
          onScan={(scanned) => {
            setCode("");
            find.mutate(scanned);
          }}
        />
      </Field>
      <div aria-live="polite" className="flex flex-col gap-1">
        {hit === null ? <p className="text-warn">{t("hardware:scale.codes.unknown")}</p> : null}
        {hit && !hit.scale ? <p>{t("hardware:scale.codes.notScale", { name })}</p> : null}
        {hit?.scale ? (
          <>
            <p className="font-semibold">
              {hit.scale.code.value === "count"
                ? t("hardware:scale.codes.resultCount", { name, qty })
                : t("hardware:scale.codes.result", { name, qty })}
            </p>
            {hit.scale.labelPrice !== null && hit.scale.till !== null ? (
              <p>
                {t("hardware:scale.codes.labelPrice", {
                  label: formatDZD(hit.scale.labelPrice, lang),
                  // both amounts as the backend computed them: the UI never computes money
                  till: formatDZD(hit.scale.till, lang),
                })}
              </p>
            ) : null}
          </>
        ) : null}
        {find.error ? (
          <p role="alert" className="text-danger">
            {errorText(find.error)}
          </p>
        ) : null}
      </div>
    </div>
  );
}
