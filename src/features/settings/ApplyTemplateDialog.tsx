import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { TemplateCards } from "@/features/setup/TemplateStep";
import { useApplyTemplate, useTemplates } from "@/lib/ipc/packs";
import type { LocalName, ShopActivity } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** "Changer d'activité…" (ONB-008): the templates, then the chosen one's modules, till screen and
 * seeds; `packs_apply_template` makes it the shop's (its modules and till screen replace the
 * hand-picked ones; seeds are created once, nothing is deleted). */
export function ApplyTemplateDialog({
  current,
  onClose,
  onApplied,
}: {
  current: ShopActivity;
  onClose: () => void;
  onApplied: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const templates = useTemplates();
  const apply = useApplyTemplate();
  const [activity, setActivity] = useState<ShopActivity>(current);
  const [seed, setSeed] = useState(true);
  const chosen = templates.data?.find((tpl) => tpl.activity === activity);
  const names = (items: readonly LocalName[]) => items.map((item) => item[lang]).join(", ");
  const submit = () =>
    apply.mutate(
      { activity, seed },
      {
        onSuccess: () => {
          onApplied();
          onClose();
        },
      },
    );
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t("packs:template.changeTitle")}
      size="lg"
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={!chosen || apply.isPending} onClick={submit}>
            {t("packs:template.apply")}
          </Button>
        </>
      }
    >
      {templates.error ? (
        <p role="alert" className="text-danger">
          {errorText(templates.error)}
        </p>
      ) : null}
      {templates.data ? (
        <TemplateCards templates={templates.data} value={activity} onChange={setActivity} />
      ) : null}
      {chosen ? (
        <section
          aria-labelledby="template-chosen"
          className="flex flex-col gap-3 rounded-card border border-line p-4"
        >
          <h3 id="template-chosen" className="text-lg font-bold">
            {t(`packs:activities.${chosen.activity}`)}
          </h3>
          <dl className="grid gap-x-4 gap-y-2 sm:grid-cols-[auto_1fr]">
            <dt className="font-semibold">{t("packs:template.tillMode")}</dt>
            <dd>{t(`packs:tillModes.${chosen.tillMode}.name`)}</dd>
            <dt className="font-semibold">{t("packs:template.modules")}</dt>
            <dd className="flex flex-wrap gap-1">
              {chosen.modules.map((module) => (
                <Badge key={module}>{t(`packs:modules.${module}.name`)}</Badge>
              ))}
            </dd>
            {chosen.categories.length > 0 ? (
              <>
                <dt className="font-semibold">{t("packs:template.categories")}</dt>
                <dd>{names(chosen.categories)}</dd>
              </>
            ) : null}
            {chosen.services.length > 0 ? (
              <>
                <dt className="font-semibold">{t("packs:template.services")}</dt>
                <dd>{names(chosen.services)}</dd>
              </>
            ) : null}
          </dl>
          <p className="text-sm text-ink-2">{t("packs:template.resets")}</p>
        </section>
      ) : null}
      <label className="flex min-h-11 items-center gap-3 font-semibold">
        <Checkbox checked={seed} onCheckedChange={(value) => setSeed(value === true)} />
        {t("packs:template.seed")}
      </label>
      {apply.error ? (
        <p role="alert" className="text-danger">
          {errorText(apply.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
