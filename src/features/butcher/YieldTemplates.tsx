import { Archive, Plus, X } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ProductPicker } from "@/features/purchases/purchase-lines";
import { formatPercentInput, parsePercentBp } from "@/lib/format";
import { useArchiveYieldTemplate, useSaveYieldTemplate } from "@/lib/ipc/butcher";
import type { YieldTemplate } from "@/lib/ipc/types";
import { formatRate } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

interface CutDraft {
  productId: string;
  name: string;
  /** The expected share of the carcass, typed in %. */
  expected: string;
}

/**
 * VRT-011: the cutting templates, a carcass product and its cuts with the share of the carcass
 * each is expected to yield (the run's preview compares them). Archived, never deleted, once a
 * run used them.
 */
export function YieldTemplates({
  templates,
  names,
  editable,
}: {
  templates: YieldTemplate[];
  names: { nameOf: (id: string) => string };
  editable: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const id = useId();
  const [open, setOpen] = useState<YieldTemplate | "new" | null>(null);
  return (
    <section className="flex flex-col gap-4" aria-labelledby={`${id}-title`}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 id={`${id}-title`} className="text-2xl font-bold">
          {t("butcher:templates.title")}
        </h2>
        {editable ? (
          <Button variant="outline" onClick={() => setOpen("new")}>
            <Plus aria-hidden />
            {t("butcher:templates.new")}
          </Button>
        ) : null}
      </div>
      {templates.length === 0 ? (
        <p className="text-ink-2">{t("butcher:templates.empty")}</p>
      ) : (
        <ul className="grid gap-3 sm:grid-cols-2">
          {templates.map((template) => (
            <li
              key={template.id}
              className="flex flex-col gap-2 rounded-card border border-line bg-surface p-4"
            >
              <div className="flex items-start justify-between gap-2">
                <div>
                  <p className="text-lg font-semibold">{template.name}</p>
                  <p className="text-sm text-ink-2">
                    {t("butcher:templates.carcassOf", { name: names.nameOf(template.sourceProductId) })}
                  </p>
                </div>
                {editable ? (
                  <Button variant="ghost" onClick={() => setOpen(template)}>
                    {t("butcher:templates.edit", { name: template.name })}
                  </Button>
                ) : null}
              </div>
              <ul className="flex flex-wrap gap-2 text-sm">
                {template.cuts.map((cut) => (
                  <li key={cut.productId} className="rounded-full bg-surface-2 px-3 py-1">
                    {names.nameOf(cut.productId)} <bdi dir="ltr">{formatRate(cut.expectedBp, lang)}</bdi>
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ul>
      )}
      {open ? (
        <TemplateDialog template={open === "new" ? null : open} names={names} onClose={() => setOpen(null)} />
      ) : null}
    </section>
  );
}

function TemplateDialog({
  template,
  names,
  onClose,
}: {
  template: YieldTemplate | null;
  names: { nameOf: (id: string) => string };
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const id = useId();
  const save = useSaveYieldTemplate();
  const archive = useArchiveYieldTemplate();
  const [name, setName] = useState(template?.name ?? "");
  const [carcass, setCarcass] = useState<{ productId: string; name: string } | null>(
    template ? { productId: template.sourceProductId, name: names.nameOf(template.sourceProductId) } : null,
  );
  const [cuts, setCuts] = useState<CutDraft[]>(
    (template?.cuts ?? []).map((c) => ({
      productId: c.productId,
      name: names.nameOf(c.productId),
      expected: formatPercentInput(c.expectedBp, lang),
    })),
  );
  const [problem, setProblem] = useState<string | null>(null);

  const submit = () => {
    const parsed = cuts.map((c) => ({ productId: c.productId, expectedBp: parsePercentBp(c.expected) }));
    if (!carcass || name.trim() === "" || parsed.length === 0 || parsed.some((c) => c.expectedBp === null)) {
      setProblem(t("butcher:templates.missing"));
      return;
    }
    setProblem(null);
    save.mutate(
      {
        id: template?.id ?? null,
        input: {
          sourceProductId: carcass.productId,
          name: name.trim(),
          cuts: parsed.map((c) => ({ productId: c.productId, expectedBp: c.expectedBp ?? 0 })),
        },
      },
      {
        onSuccess: () => {
          toast.success(t("butcher:templates.saved"));
          onClose();
        },
      },
    );
  };
  const archiveIt = () => {
    if (!template) return;
    archive.mutate(template.id, {
      onSuccess: () => {
        toast.success(t("butcher:templates.archived"));
        onClose();
      },
    });
  };
  const error = save.error ?? archive.error;
  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={template ? t("butcher:templates.edit", { name: template.name }) : t("butcher:templates.new")}
      footer={
        <>
          {template ? (
            <Button variant="danger" disabled={archive.isPending} onClick={archiveIt}>
              <Archive aria-hidden />
              {t("butcher:templates.archive")}
            </Button>
          ) : null}
          <Button variant="primary" disabled={save.isPending} onClick={submit}>
            {t("butcher:templates.save")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <Field id={`${id}-name`} label={t("butcher:templates.name")}>
          <Input
            id={`${id}-name`}
            dir="auto"
            maxLength={80}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </Field>
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 text-lg font-semibold">{t("butcher:templates.carcass")}</legend>
          {carcass ? (
            <p className="font-semibold">{t("butcher:templates.carcassOf", { name: carcass.name })}</p>
          ) : null}
          <ProductPicker onPick={(p) => setCarcass({ productId: p.productId, name: p.name })} />
        </fieldset>
        <fieldset className="flex flex-col gap-3">
          <legend className="mb-1 text-lg font-semibold">{t("butcher:templates.cuts")}</legend>
          <ul className="flex flex-col gap-2">
            {cuts.map((cut) => (
              <li key={cut.productId} className="flex flex-wrap items-end gap-3">
                <span className="min-w-40 flex-1 font-semibold">{cut.name}</span>
                <Field id={`${id}-cut-${cut.productId}`} label={t("butcher:templates.expected")}>
                  <NumericInput
                    id={`${id}-cut-${cut.productId}`}
                    aria-label={t("butcher:templates.expectedOf", { name: cut.name })}
                    className="w-32"
                    value={cut.expected}
                    onValueChange={(expected) =>
                      setCuts((all) =>
                        all.map((c) => (c.productId === cut.productId ? { ...c, expected } : c)),
                      )
                    }
                  />
                </Field>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t("butcher:templates.remove", { name: cut.name })}
                  onClick={() => setCuts((all) => all.filter((c) => c.productId !== cut.productId))}
                >
                  <X aria-hidden />
                </Button>
              </li>
            ))}
          </ul>
          <ProductPicker
            onPick={(p) =>
              setCuts((all) =>
                all.some((c) => c.productId === p.productId)
                  ? all
                  : [...all, { productId: p.productId, name: p.name, expected: "" }],
              )
            }
          />
        </fieldset>
        {problem ? (
          <p role="alert" className="text-danger">
            {problem}
          </p>
        ) : null}
        {error ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
      </div>
    </Dialog>
  );
}
