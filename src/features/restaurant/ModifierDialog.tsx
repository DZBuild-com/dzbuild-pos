import type { TFunction } from "i18next";
import { type KeyboardEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useProduct } from "@/lib/ipc/products";
import type { ModifierGroup, ModifierOption } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** The rule of a group as the waiter reads it: "Choisir 1", "Jusqu'à 3", "De 1 à 3". */
function ruleOf(group: ModifierGroup, t: TFunction) {
  if (group.minSelect === group.maxSelect)
    return t("restaurant:modifiers.choose", { count: group.minSelect });
  if (group.minSelect === 0) return t("restaurant:modifiers.upTo", { count: group.maxSelect });
  return t("restaurant:modifiers.range", { min: group.minSelect, max: group.maxSelect });
}

/** SAL-024, PRD-021: the options of a product, one section per group with its rule, as 56 px
 * toggle tiles (a priced option shows its product's price); "Ajouter" waits until every forced
 * group has its minimum. With the keyboard (spec §10.5): the first option has the focus, the
 * arrows move between the options (the screen's direction), Space picks one, Enter adds. */
export function ModifierDialog({
  title,
  groups,
  busy,
  onAdd,
  onClose,
}: {
  /** The product's name. */
  title: string;
  groups: ModifierGroup[];
  busy: boolean;
  onAdd: (optionIds: string[]) => void;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = useLang();
  const [chosen, setChosen] = useState<Record<string, string[]>>({});
  const live = groups.filter((g) => !g.archived);
  const picked = (group: ModifierGroup) => chosen[group.id] ?? [];
  const toggle = (group: ModifierGroup, option: ModifierOption) =>
    setChosen((now) => {
      const current = now[group.id] ?? [];
      const next = current.includes(option.id)
        ? current.filter((id) => id !== option.id)
        : group.maxSelect === 1
          ? [option.id]
          : current.length < group.maxSelect
            ? [...current, option.id]
            : current;
      return { ...now, [group.id]: next };
    });
  const complete = live.every((g) => picked(g).length >= g.minSelect && picked(g).length <= g.maxSelect);
  const add = () => {
    if (complete && !busy) onAdd(live.flatMap((g) => picked(g)));
  };
  const firstOption = live.flatMap((g) => g.options.filter((o) => !o.archived))[0]?.id;
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Enter") {
      // an option keeps Space; Enter is "Ajouter" from anywhere among them
      event.preventDefault();
      add();
      return;
    }
    const rtl = i18n.dir() === "rtl";
    const step =
      event.key === "ArrowDown" || event.key === (rtl ? "ArrowLeft" : "ArrowRight")
        ? 1
        : event.key === "ArrowUp" || event.key === (rtl ? "ArrowRight" : "ArrowLeft")
          ? -1
          : 0;
    const options = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button[aria-pressed]")];
    const index = options.findIndex((option) => option === event.target);
    if (step === 0 || index < 0) return;
    event.preventDefault();
    options[(index + step + options.length) % options.length]?.focus();
  };
  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={title}
      footer={
        <>
          <Button variant="outline" size="lg" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            disabled={!complete || busy}
            onClick={add}
            shortcut="Enter"
            shortcutLabel={t("common:keys.enter")}
          >
            {t("restaurant:modifiers.add")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4" onKeyDown={onKeyDown}>
        {live.map((group) => {
          const count = picked(group).length;
          const missing = count < group.minSelect;
          return (
            <section key={group.id} aria-labelledby={`group-${group.id}`} className="flex flex-col gap-2">
              <div className="flex flex-wrap items-baseline gap-x-3">
                <h3 id={`group-${group.id}`} className="text-lg font-bold">
                  {localizedName(group, lang)}
                </h3>
                <span className={cn("text-sm", missing ? "font-semibold text-danger" : "text-ink-2")}>
                  {ruleOf(group, t)}
                  {group.minSelect > 0 ? ` · ${t("restaurant:modifiers.required")}` : ""}
                </span>
              </div>
              <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                {group.options
                  .filter((o) => !o.archived)
                  .map((option) => {
                    const on = picked(group).includes(option.id);
                    return (
                      <button
                        key={option.id}
                        type="button"
                        aria-pressed={on}
                        autoFocus={option.id === firstOption}
                        onClick={() => toggle(group, option)}
                        className={cn(
                          "flex min-h-14 flex-col items-start justify-center rounded-control border-2 px-3 text-start font-semibold",
                          on ? "border-brand bg-brand-soft" : "border-line bg-surface-2",
                        )}
                      >
                        <span>{localizedName(option, lang)}</span>
                        {option.productId ? <OptionPrice productId={option.productId} /> : null}
                      </button>
                    );
                  })}
              </div>
            </section>
          );
        })}
      </div>
    </Dialog>
  );
}

/** A priced option is a product of its own: its catalogue price. */
function OptionPrice({ productId }: { productId: string }) {
  const product = useProduct(productId);
  return product.data ? (
    <Amount value={product.data.sellPrice} className="text-sm font-normal text-ink-2" />
  ) : null;
}
