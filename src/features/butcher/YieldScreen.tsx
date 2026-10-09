import { CheckCircle2, Scale } from "lucide-react";
import { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { useCan } from "@/features/stock/stock-parts";
import { formatDZD, formatPercentInput } from "@/lib/format";
import { useYieldPreview, useYieldRun, useYieldTemplates } from "@/lib/ipc/butcher";
import { useDeviceStatus, useReadStable } from "@/lib/ipc/devices";
import type { CostMethod, CutResult, RunInput, YieldRun, YieldTemplate } from "@/lib/ipc/types";
import { formatQty, formatRate, parseAmount, parseQty } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { useProductNames } from "./names";
import { YieldTemplates } from "./YieldTemplates";

/** Weights are kilograms to the gram. */
const KG_DECIMALS = 3;
/** How long "Peser" waits for a stable weight. */
const WEIGH_TIMEOUT_MS = 3_000;
const METHODS: readonly CostMethod[] = ["value", "weight"];

const col = columnHelper<CutResult>();

/** The run the form describes, or `null` while it lacks a template, a weight or a cut's weight
 * (or a typed cost is not an amount). */
function runInput(
  template: YieldTemplate | null,
  weight: string,
  cost: string,
  method: CostMethod,
  cutWeights: Record<string, string>,
): RunInput | null {
  if (!template) return null;
  const inputQty = parseQty(weight, KG_DECIMALS);
  const inputCost = cost.trim() === "" ? null : parseAmount(cost);
  if (inputQty === null || (cost.trim() !== "" && inputCost === null)) return null;
  const cuts = template.cuts.flatMap((c) => {
    const qty = parseQty(cutWeights[c.productId] ?? "", KG_DECIMALS);
    return qty === null ? [] : [{ productId: c.productId, qty }];
  });
  if (cuts.length === 0) return null;
  return {
    templateId: template.id,
    sourceProductId: template.sourceProductId,
    locationId: null,
    inputQty,
    inputCost,
    method,
    cuts,
    note: null,
  };
}

/**
 * VRT-011: a carcass cut up. The template names the carcass and its cuts; the butcher types (or
 * weighs, when this PC's scale is connected) the carcass and each cut, may type the carcass's
 * cost (its CMUP × weight otherwise) and how to share it. The backend previews the run live (each
 * cut's yield, cost and cost per kg, and the loss) and "Valider la découpe" records it under its
 * `DC…` number. The templates are edited below.
 */
export function YieldScreen() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const errorText = useErrorText();
  const id = useId();
  const templates = useYieldTemplates();
  const [templateId, setTemplateId] = useState("");
  const [weight, setWeight] = useState("");
  const [cost, setCost] = useState("");
  const [method, setMethod] = useState<CostMethod>("value");
  const [cutWeights, setCutWeights] = useState<Record<string, string>>({});
  const [done, setDone] = useState<YieldRun | null>(null);
  const list = useMemo(() => templates.data ?? [], [templates.data]);
  const template = list.find((x) => x.id === templateId) ?? null;
  // Every name the templates need, read once: picking a template shows its cuts at once.
  const productIds = useMemo(
    () => list.flatMap((x) => [x.sourceProductId, ...x.cuts.map((c) => c.productId)]),
    [list],
  );
  const names = useProductNames(productIds);
  const input = useMemo(
    () => runInput(template, weight, cost, method, cutWeights),
    [template, weight, cost, method, cutWeights],
  );
  const preview = useYieldPreview(useDebounced(input, 300));
  const run = useYieldRun();
  const devices = useDeviceStatus();
  const scaleOn = (devices.data ?? []).some((d) => d.device === "scale" && d.state.state === "connected");
  const readStable = useReadStable();
  const seesCosts = can("reports.profit");
  const adjusts = can("stock.adjust");

  const weigh = (set: (value: string) => void) =>
    readStable.mutate(WEIGH_TIMEOUT_MS, { onSuccess: (reading) => set(formatQty(reading.net, lang)) });
  const reset = () => {
    setDone(null);
    setWeight("");
    setCost("");
    setCutWeights({});
    run.reset();
  };
  const validate = () => {
    if (!input) return;
    run.mutate(input, { onSuccess: setDone });
  };

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("name", { header: () => t("butcher:yield.columns.cut") }),
        col.accessor("qty", {
          header: () => t("butcher:yield.columns.weight"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatQty(c.getValue(), lang)}</bdi>,
        }),
        col.accessor("expectedQty", {
          header: () => t("butcher:yield.columns.expected"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatQty(c.getValue(), lang)}</bdi>,
        }),
        col.accessor("yieldBp", {
          header: () => t("butcher:yield.columns.yield"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatRate(c.getValue(), lang)}</bdi>,
        }),
        ...(seesCosts
          ? [
              col.accessor("cost", {
                header: () => t("butcher:yield.columns.cost"),
                meta: { align: "end" },
                cell: (c) => <Amount value={c.getValue()} />,
              }),
              col.accessor("unitCost", {
                header: () => t("butcher:yield.columns.unitCost"),
                meta: { align: "end" },
                cell: (c) => <Amount value={c.getValue()} />,
              }),
            ]
          : []),
      ]),
    [t, lang, seesCosts],
  );

  if (templates.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(templates.error)}
      </p>
    );
  }
  if (!templates.data) return null;
  const shown = input && preview.data ? preview.data : null;
  return (
    <div className="flex flex-col gap-8">
      {adjusts ? (
        <section className="flex flex-col gap-4" aria-labelledby={`${id}-title`}>
          <h2 id={`${id}-title`} className="text-2xl font-bold">
            {t("butcher:yield.title")}
          </h2>
          {done ? (
            <div
              role="status"
              className="flex flex-col items-start gap-3 rounded-card border border-line bg-surface p-4"
            >
              <p className="flex items-center gap-2 text-lg font-semibold text-ok">
                <CheckCircle2 aria-hidden className="size-6" />
                {t("butcher:yield.doneTitle")}
              </p>
              <p className="flex flex-wrap items-center gap-2">
                <span className="text-ink-2">{t("butcher:yield.number")}</span>
                <bdi dir="ltr" className="text-2xl font-bold">
                  {done.number}
                </bdi>
              </p>
              <Button variant="secondary" onClick={reset}>
                {t("butcher:yield.again")}
              </Button>
            </div>
          ) : list.length === 0 ? (
            <p className="text-ink-2">{t("butcher:yield.noTemplate")}</p>
          ) : (
            <>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field id={`${id}-template`} label={t("butcher:yield.template")}>
                  <NativeSelect
                    id={`${id}-template`}
                    value={templateId}
                    onChange={(event) => {
                      setTemplateId(event.target.value);
                      setCutWeights({});
                    }}
                  >
                    <option value="">{t("butcher:yield.choose")}</option>
                    {list.map((x) => (
                      <option key={x.id} value={x.id}>
                        {x.name}
                      </option>
                    ))}
                  </NativeSelect>
                </Field>
                {template ? (
                  <p className="self-end pb-2 font-semibold">
                    {t("butcher:yield.carcass", { name: names.nameOf(template.sourceProductId) })}
                  </p>
                ) : null}
                <WeightField
                  id={`${id}-weight`}
                  label={t("butcher:yield.weight")}
                  weighLabel={t("butcher:yield.weighOf", { name: t("butcher:yield.weight") })}
                  value={weight}
                  onChange={setWeight}
                  onWeigh={scaleOn ? () => weigh(setWeight) : undefined}
                />
                <Field id={`${id}-cost`} label={t("butcher:yield.cost")} hint={t("butcher:yield.costHint")}>
                  <NumericInput id={`${id}-cost`} value={cost} onValueChange={setCost} />
                </Field>
              </div>
              <RadioCards<CostMethod>
                legend={t("butcher:yield.method")}
                name={`${id}-method`}
                value={method}
                onChange={setMethod}
                options={METHODS.map((m) => ({ value: m, label: t(`butcher:yield.methods.${m}`) }))}
              />
              {template && names.ready ? (
                <fieldset className="flex flex-col gap-3">
                  <legend className="mb-1 text-lg font-semibold">{t("butcher:yield.cuts")}</legend>
                  <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
                    {template.cuts.map((c) => {
                      const name = names.nameOf(c.productId);
                      return (
                        <WeightField
                          key={c.productId}
                          id={`${id}-cut-${c.productId}`}
                          label={t("butcher:yield.cutWeight", { name })}
                          weighLabel={t("butcher:yield.weighOf", { name })}
                          value={cutWeights[c.productId] ?? ""}
                          onChange={(value) => setCutWeights((all) => ({ ...all, [c.productId]: value }))}
                          onWeigh={
                            scaleOn
                              ? () =>
                                  weigh((value) => setCutWeights((all) => ({ ...all, [c.productId]: value })))
                              : undefined
                          }
                        />
                      );
                    })}
                  </div>
                </fieldset>
              ) : null}
              {readStable.error ? (
                <p role="alert" className="text-danger">
                  {errorText(readStable.error)}
                </p>
              ) : null}
              {shown ? (
                <section className="flex flex-col gap-3" aria-labelledby={`${id}-preview`}>
                  <h3 id={`${id}-preview`} className="text-xl font-semibold">
                    {t("butcher:yield.preview")}
                  </h3>
                  <DataTable
                    ariaLabel={t("butcher:yield.preview")}
                    data={shown.cuts}
                    columns={columns}
                    getRowId={(c) => c.productId}
                    sortable={false}
                  />
                  <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1">
                    <p className="text-xl font-bold">
                      {t("butcher:yield.loss", { rate: formatPercentInput(shown.lossBp, lang) })}
                    </p>
                    <p className="text-ink-2">
                      {t("butcher:yield.lossQty", { qty: formatQty(shown.lossQty, lang) })}
                    </p>
                    {seesCosts ? (
                      <p className="font-semibold">
                        {t("butcher:yield.inputCost", { amount: formatDZD(shown.inputCost, lang) })}
                      </p>
                    ) : null}
                  </div>
                </section>
              ) : null}
              {preview.error && input ? (
                <p role="alert" className="text-danger">
                  {errorText(preview.error)}
                </p>
              ) : null}
              {run.error ? (
                <p role="alert" className="text-danger">
                  {errorText(run.error)}
                </p>
              ) : null}
              <div className="flex justify-end">
                <Button variant="primary" disabled={!input || run.isPending} onClick={validate}>
                  {t("butcher:yield.run")}
                </Button>
              </div>
            </>
          )}
        </section>
      ) : null}
      <YieldTemplates templates={list} names={names} editable={adjusts} />
    </div>
  );
}

/** A weight in kilograms, typed or read from the scale ("Peser", when it is connected). */
function WeightField({
  id,
  label,
  weighLabel,
  value,
  onChange,
  onWeigh,
}: {
  id: string;
  label: string;
  weighLabel: string;
  value: string;
  onChange: (value: string) => void;
  onWeigh?: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Field id={id} label={label}>
      <div className="flex gap-2">
        <NumericInput id={id} kind="qty" value={value} onValueChange={onChange} />
        {onWeigh ? (
          <Button variant="secondary" aria-label={weighLabel} onClick={onWeigh}>
            <Scale aria-hidden />
            {t("butcher:yield.weigh")}
          </Button>
        ) : null}
      </div>
    </Field>
  );
}
