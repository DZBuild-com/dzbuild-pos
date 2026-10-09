import { ArrowDown, ArrowUp, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useProduct } from "@/lib/ipc/products";
import { useDailyMenu, useSetDailyMenu } from "@/lib/ipc/restaurant";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { ProductPicker } from "./ProductPicker";

/** Today on this PC, `YYYY-MM-DD`. */
function today(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** The products of a day's menu (menu du jour), in their order: the ticket screen shows them first. */
export function DailyMenuEditor() {
  const { t } = useTranslation();
  const [day, setDay] = useState(today);
  return (
    <div className="flex flex-col gap-4">
      <Field id="menu-day" label={t("restaurant:editor.day")}>
        <Input
          id="menu-day"
          type="date"
          dir="ltr"
          className="sm:w-56"
          value={day}
          onChange={(e) => (e.target.value ? setDay(e.target.value) : undefined)}
        />
      </Field>
      <MenuOfDay key={day} day={day} />
    </div>
  );
}

function MenuOfDay({ day }: { day: string }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const menu = useDailyMenu(day);
  const set = useSetDailyMenu();
  const [draft, setDraft] = useState<string[] | null>(null);
  const ids = draft ?? menu.data ?? [];
  const move = (index: number, delta: -1 | 1) => {
    const next = [...ids];
    const [item] = next.splice(index, 1);
    if (item === undefined) return;
    next.splice(index + delta, 0, item);
    setDraft(next);
  };
  return (
    <div className="flex flex-col gap-3">
      {ids.length === 0 && menu.isSuccess ? (
        <p className="text-ink-2">{t("restaurant:editor.menuEmpty")}</p>
      ) : null}
      <ol className="flex flex-col gap-1">
        {ids.map((id, index) => (
          <MenuItem
            key={id}
            productId={id}
            first={index === 0}
            last={index === ids.length - 1}
            onUp={() => move(index, -1)}
            onDown={() => move(index, 1)}
            onRemove={() => setDraft(ids.filter((x) => x !== id))}
          />
        ))}
      </ol>
      <div className="sm:w-96">
        <ProductPicker
          id="menu-add"
          label={t("restaurant:editor.addToMenu")}
          onPick={(hit) => (ids.includes(hit.id) ? undefined : setDraft([...ids, hit.id]))}
        />
      </div>
      <Button
        variant="primary"
        className="self-start"
        disabled={draft === null || set.isPending}
        onClick={() => set.mutate({ day, productIds: ids }, { onSuccess: () => setDraft(null) })}
      >
        {t("restaurant:editor.save")}
      </Button>
      {set.error ? (
        <p role="alert" className="text-danger">
          {errorText(set.error)}
        </p>
      ) : null}
    </div>
  );
}

function MenuItem({
  productId,
  first,
  last,
  onUp,
  onDown,
  onRemove,
}: {
  productId: string;
  first: boolean;
  last: boolean;
  onUp: () => void;
  onDown: () => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const product = useProduct(productId);
  const name = product.data ? localizedName(product.data, lang) : "";
  return (
    <li className="flex min-h-11 items-center gap-2 rounded-control border border-line bg-surface px-3">
      <span className="flex-1 font-semibold">{name}</span>
      {product.data ? <Amount value={product.data.sellPrice} /> : null}
      <Button
        variant="ghost"
        size="icon"
        disabled={first}
        aria-label={t("restaurant:editor.up", { name })}
        onClick={onUp}
      >
        <ArrowUp aria-hidden />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        disabled={last}
        aria-label={t("restaurant:editor.down", { name })}
        onClick={onDown}
      >
        <ArrowDown aria-hidden />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("restaurant:editor.removeFromMenu", { name })}
        onClick={onRemove}
      >
        <X aria-hidden />
      </Button>
    </li>
  );
}
