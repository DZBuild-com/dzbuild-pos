import { zodResolver } from "@hookform/resolvers/zod";
import { useEffect, useState } from "react";
import { Controller, useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { z } from "zod";
import { NumericInput } from "@/components/numeric-input";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { useDiscoverDevices, useHardwareConfig, useSaveHardware } from "@/lib/ipc/hardware";
import type { DocType, HardwareConfig, KindPrinting, Paper } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

/** The A4/A5 kinds a shop prints (spec §9.2), in the order of the screen. */
const KINDS = [
  "invoice",
  "quote",
  "proforma",
  "delivery_note",
  "credit_note",
  "purchase_order",
  "transfer",
] as const satisfies readonly DocType[];
const PAPERS = ["a4", "a5"] as const satisfies readonly Paper[];

/** pos-hw `DocumentPrinting::validate` limits. */
const printersSchema = z.object({
  defaultPrinter: z.string(),
  marginsMm: z.string().refine((v) => /^\d{1,2}$/.test(v) && Number(v) <= 25, "validation:marginsRange"),
  kinds: z.array(
    z.object({
      docType: z.enum(KINDS),
      printer: z.string(),
      paper: z.enum(PAPERS),
      copies: z.string().refine((v) => /^[1-5]$/.test(v), "validation:copiesRange"),
    }),
  ),
});

type PrintersForm = z.infer<typeof printersSchema>;

const isScreenKind = (docType: DocType) => (KINDS as readonly DocType[]).includes(docType);

/** `hardware.json` → the form: a kind without its own line prints on the default printer, on the
 * paper of the document settings, once. */
function toForm(config: HardwareConfig, defaultPaper: Paper): PrintersForm {
  const documents = config.documents ?? {};
  return {
    defaultPrinter: documents.defaultPrinter ?? "",
    marginsMm: String(documents.marginsMm ?? 12),
    kinds: KINDS.map((docType) => {
      const own = documents.kinds?.find((k) => k.docType === docType);
      return {
        docType,
        printer: own?.printer ?? "",
        paper: own?.paper ?? defaultPaper,
        copies: String(own?.copies ?? 1),
      };
    }),
  };
}

/** The form → `hardware.json`: only the kinds that differ from the defaults get a line; the kinds
 * this screen does not show keep theirs. */
function toConfig(config: HardwareConfig, v: PrintersForm, defaultPaper: Paper): HardwareConfig {
  const shown: KindPrinting[] = v.kinds
    .filter((k) => k.printer !== "" || k.paper !== defaultPaper || Number(k.copies) !== 1)
    .map((k) => ({
      docType: k.docType,
      printer: k.printer === "" ? null : k.printer,
      paper: k.paper,
      copies: Number(k.copies),
    }));
  const others = (config.documents?.kinds ?? []).filter((k) => !isScreenKind(k.docType));
  return {
    ...config,
    documents: {
      defaultPrinter: v.defaultPrinter === "" ? null : v.defaultPrinter,
      marginsMm: Number(v.marginsMm),
      kinds: [...shown, ...others],
    },
  };
}

/** The Windows printers of this PC by name, plus those already chosen that Windows no longer
 * lists (unplugged, renamed), so a saved choice is never silently replaced. */
function printerNames(found: string[], config: HardwareConfig): string[] {
  const chosen = [config.documents?.defaultPrinter, ...(config.documents?.kinds ?? []).map((k) => k.printer)];
  const names = [...found, ...chosen.filter((n): n is string => typeof n === "string" && n !== "")];
  return [...new Set(names)];
}

/** Réglages › Documents: the A4 printer, margins and, per document type, printer, paper and
 * copies of this PC (`hardware.json`, never the shared database: each station has its own). */
export function DocumentPrintersCard({ defaultPaper }: { defaultPaper: Paper }) {
  const errorText = useErrorText();
  const config = useHardwareConfig();
  const discover = useDiscoverDevices();
  const { mutate } = discover;
  // Windows is asked for its printers once, when the card opens.
  useEffect(() => {
    mutate();
  }, [mutate]);
  if (!config.data || discover.isIdle || discover.isPending) return <Splash inline />;
  const found = (discover.data ?? []).flatMap((d) => (d.kind === "windows_printer" ? [d.name] : []));
  return (
    <PrintersFormView
      config={config.data}
      names={printerNames(found, config.data)}
      defaultPaper={defaultPaper}
      discoverError={discover.error ? errorText(discover.error) : undefined}
    />
  );
}

function PrintersFormView({
  config,
  names,
  defaultPaper,
  discoverError,
}: {
  config: HardwareConfig;
  names: string[];
  defaultPaper: Paper;
  discoverError: string | undefined;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const save = useSaveHardware();
  const [failure, setFailure] = useState<unknown>(null);
  const {
    control,
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm<PrintersForm>({
    resolver: zodResolver(printersSchema),
    defaultValues: toForm(config, defaultPaper),
  });
  const err = (message: string | undefined) => (message ? t(message) : undefined);

  const onSubmit = handleSubmit(async (values) => {
    setFailure(null);
    try {
      await save.mutateAsync(toConfig(config, values, defaultPaper));
      toast.success(t("settings:documents.printers.saved"));
    } catch (error) {
      setFailure(error);
    }
  });

  const printerOptions = (none: string) => (
    <>
      <option value="">{none}</option>
      {names.map((name) => (
        <option key={name} value={name}>
          {name}
        </option>
      ))}
    </>
  );

  return (
    <Card>
      <form noValidate onSubmit={(event) => void onSubmit(event)} className="flex flex-col gap-4">
        <h2 className="text-xl font-bold">{t("settings:documents.printers.title")}</h2>
        <p className="text-sm text-ink-2">{t("settings:documents.printers.hint")}</p>
        {discoverError ? (
          <p role="alert" className="rounded-control bg-warn-soft p-3 text-warn">
            {discoverError}
          </p>
        ) : null}
        <div className="grid gap-4 md:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
          <Field id="documents-default-printer" label={t("settings:documents.printers.default")}>
            <NativeSelect id="documents-default-printer" {...register("defaultPrinter")}>
              {printerOptions(t("settings:documents.printers.windowsDefault"))}
            </NativeSelect>
          </Field>
          <Field
            id="documents-margins"
            label={t("settings:documents.printers.margins")}
            error={err(errors.marginsMm?.message)}
          >
            <Controller
              name="marginsMm"
              control={control}
              render={({ field }) => (
                <NumericInput
                  id="documents-margins"
                  kind="integer"
                  maxLength={2}
                  aria-invalid={Boolean(errors.marginsMm)}
                  value={field.value}
                  onValueChange={field.onChange}
                />
              )}
            />
          </Field>
        </div>
        {KINDS.map((docType, index) => {
          const kind = t(`billing:kinds.${docType}`);
          const id = `documents-${docType}`;
          return (
            <div
              key={docType}
              className="grid gap-3 border-t border-line pt-4 md:grid-cols-[minmax(0,2fr)_minmax(0,1fr)_minmax(0,1fr)]"
            >
              <Field id={`${id}-printer`} label={t("settings:documents.printers.printerFor", { kind })}>
                <NativeSelect id={`${id}-printer`} {...register(`kinds.${index}.printer`)}>
                  {printerOptions(t("settings:documents.printers.default"))}
                </NativeSelect>
              </Field>
              <Field id={`${id}-paper`} label={t("settings:documents.printers.paperFor", { kind })}>
                <NativeSelect id={`${id}-paper`} {...register(`kinds.${index}.paper`)}>
                  {PAPERS.map((paper) => (
                    <option key={paper} value={paper}>
                      {t(`settings:documents.layout.papers.${paper}`)}
                    </option>
                  ))}
                </NativeSelect>
              </Field>
              <Field
                id={`${id}-copies`}
                label={t("settings:documents.printers.copiesFor", { kind })}
                error={err(errors.kinds?.[index]?.copies?.message)}
              >
                <Controller
                  name={`kinds.${index}.copies`}
                  control={control}
                  render={({ field }) => (
                    <NumericInput
                      id={`${id}-copies`}
                      kind="integer"
                      maxLength={1}
                      aria-invalid={Boolean(errors.kinds?.[index]?.copies)}
                      value={field.value}
                      onValueChange={field.onChange}
                    />
                  )}
                />
              </Field>
            </div>
          );
        })}
        {failure ? (
          <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
            {errorText(failure)}
          </p>
        ) : null}
        <div className="flex justify-end">
          <Button type="submit" disabled={isSubmitting}>
            {t("settings:documents.printers.save")}
          </Button>
        </div>
      </form>
    </Card>
  );
}
