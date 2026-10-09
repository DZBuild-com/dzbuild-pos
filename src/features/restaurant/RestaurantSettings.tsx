import { X } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { LanguageToggle } from "@/components/language-toggle";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { isLang } from "@/lib/i18n";
import { useProduct } from "@/lib/ipc/products";
import { type SettingKey, useSetSetting, useSetting } from "@/lib/ipc/settings";
import type { JsonValue } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { ProductPicker } from "./ProductPicker";

/** `restaurant.predefined_tickets`' limits (the backend checks them again). */
const MAX_LABELS = 30;
const MAX_LABEL_CHARS = 40;

/**
 * `/restaurant` › Réglages (final review, `settings.company`): the restaurant's own settings —
 * the language kitchen tickets print in, the counter's predefined tickets, whether unsent lines go
 * to the kitchen when a ticket is paid, and the product sold per cover.
 */
export function RestaurantSettings() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const save = useSetSetting();
  const kitchenLang = useSetting("restaurant.kitchen_lang").data;
  const labels = useSetting("restaurant.predefined_tickets").data;
  const sendOnPay = useSetting("restaurant.send_on_pay").data;
  const coverId = useSetting("restaurant.cover_product_id").data;
  const cover = useProduct(typeof coverId === "string" ? coverId : null);
  const list = Array.isArray(labels) ? labels.filter((l): l is string => typeof l === "string") : [];
  const [label, setLabel] = useState("");
  const set = (key: SettingKey, value: JsonValue) => save.mutate({ key, value });
  const trimmed = label.trim();
  const canAdd =
    trimmed.length > 0 &&
    [...trimmed].length <= MAX_LABEL_CHARS &&
    list.length < MAX_LABELS &&
    !list.includes(trimmed);
  const add = (event: FormEvent) => {
    event.preventDefault();
    if (!canAdd) return;
    set("restaurant.predefined_tickets", [...list, trimmed]);
    setLabel("");
  };
  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <section className="flex flex-col gap-2" aria-labelledby="kitchen-lang">
        <h2 id="kitchen-lang" className="text-xl font-bold">
          {t("restaurant:settings.kitchenLang")}
        </h2>
        <p className="text-ink-2">{t("restaurant:settings.kitchenLangHelp")}</p>
        <LanguageToggle
          value={isLang(kitchenLang) ? kitchenLang : "fr"}
          onChange={(value) => set("restaurant.kitchen_lang", value)}
        />
      </section>

      <section className="flex flex-col gap-2" aria-labelledby="counter-tickets">
        <h2 id="counter-tickets" className="text-xl font-bold">
          {t("restaurant:settings.counterTickets")}
        </h2>
        <p className="text-ink-2">{t("restaurant:settings.counterTicketsHelp")}</p>
        {list.length === 0 ? <p className="text-ink-2">{t("restaurant:settings.noCounterTickets")}</p> : null}
        <ul className="flex flex-wrap gap-2">
          {list.map((item) => (
            <li
              key={item}
              className="flex min-h-11 items-center gap-2 rounded-control border border-line bg-surface ps-3 pe-1"
            >
              <span className="font-semibold">{item}</span>
              <Button
                variant="ghost"
                size="icon"
                aria-label={t("restaurant:settings.removeCounterTicket", { label: item })}
                onClick={() =>
                  set(
                    "restaurant.predefined_tickets",
                    list.filter((l) => l !== item),
                  )
                }
              >
                <X aria-hidden />
              </Button>
            </li>
          ))}
        </ul>
        <form className="flex flex-wrap items-end gap-2" onSubmit={add}>
          <div className="min-w-56 flex-1">
            <Field id="counter-ticket-new" label={t("restaurant:settings.newCounterTicket")}>
              <Input
                id="counter-ticket-new"
                value={label}
                maxLength={MAX_LABEL_CHARS}
                onChange={(event) => setLabel(event.target.value)}
              />
            </Field>
          </div>
          <Button type="submit" variant="secondary" disabled={!canAdd}>
            {t("restaurant:settings.addCounterTicket")}
          </Button>
        </form>
      </section>

      <section className="flex flex-col gap-2" aria-labelledby="send-on-pay">
        <h2 id="send-on-pay" className="text-xl font-bold">
          {t("restaurant:settings.kitchen")}
        </h2>
        <label className="flex min-h-11 items-center gap-3">
          <Checkbox
            checked={sendOnPay !== false}
            onCheckedChange={(checked) => set("restaurant.send_on_pay", checked === true)}
          />
          <span>{t("restaurant:settings.sendOnPay")}</span>
        </label>
      </section>

      <section className="flex flex-col gap-2" aria-labelledby="cover-product">
        <h2 id="cover-product" className="text-xl font-bold">
          {t("restaurant:settings.cover")}
        </h2>
        <p className="text-ink-2">{t("restaurant:settings.coverHelp")}</p>
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-semibold">
            {typeof coverId !== "string"
              ? t("restaurant:settings.noCover")
              : cover.data
                ? localizedName(cover.data, lang)
                : t("common:status.loading")}
          </span>
          {typeof coverId === "string" ? (
            <Button variant="outline" onClick={() => set("restaurant.cover_product_id", null)}>
              {t("restaurant:settings.removeCover")}
            </Button>
          ) : null}
        </div>
        <ProductPicker
          id="cover-product-pick"
          label={t("restaurant:settings.pickCover")}
          onPick={(hit) => set("restaurant.cover_product_id", hit.id)}
        />
      </section>

      {save.error ? (
        <p role="alert" className="text-danger">
          {errorText(save.error)}
        </p>
      ) : null}
    </div>
  );
}
