import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { backOfficeHome } from "@/app/nav";
import { LanguageToggle } from "@/components/language-toggle";
import { Button } from "@/components/ui/button";
import { NativeSelect } from "@/components/ui/native-select";
import { Sheet } from "@/components/ui/sheet";
import { useLogout } from "@/lib/ipc/auth";
import { usePriceLevels } from "@/lib/ipc/catalog";
import type { Permission } from "@/lib/ipc/types";
import { ariaKey, keyLabel } from "@/lib/key-map";
import { localizedName } from "@/lib/names";
import { useLang } from "@/lib/use-language";
import { useCart } from "./cart-store";
import type { PosContext } from "./PosDialogs";

/** Esc: everything the F-keys do not cover (SAL-018, SAL-041, PRC-005, UX-004 document language,
 * SAL-046 a quote loaded at the till). */
export function PosMenu({ ctx, onClose }: { ctx: PosContext; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const logout = useLogout();
  const levels = usePriceLevels();
  const priceLevelId = useCart((s) => s.priceLevelId);
  const docLang = useCart((s) => s.lang);
  const hasLines = useCart((s) => s.lines.length > 0);
  const exchanging = useCart((s) => s.exchange !== null);
  const can = (permission: Permission) => ctx.session.permissions.includes(permission);
  const [confirmCancel, setConfirmCancel] = useState(false);
  const go = (path: string) => {
    onClose();
    void navigate(path);
  };
  const item = "justify-start";
  // M8 packs on (the till's pack state): an instalment contract from the cart (TMP-001), the cart
  // put on order (ORD-002/003), a gift voucher (LOY-001), a top-up, the department keys, the van
  const on = (module: (typeof ctx.modules)[number]) => ctx.modules.includes(module);
  return (
    <Sheet open onOpenChange={(open) => (open ? undefined : onClose())} title={t("pos:menu.title")}>
      <div className="flex flex-col gap-3">
        <Button className={item} onClick={() => ctx.open({ kind: "openItem" })}>
          {t("pos:menu.openItem")}
        </Button>
        {ctx.session.permissions.includes("sales.create") ? (
          <Button
            className={item}
            onClick={() => ctx.open({ kind: "quote" })}
            shortcut="Control+D"
            shortcutLabel={t("common:keys.ctrlD")}
          >
            {t("billing:offer.menu")}
          </Button>
        ) : null}
        {on("instalments") && can("sales.create") ? (
          <Button className={item} disabled={exchanging} onClick={() => ctx.open({ kind: "instalment" })}>
            {t("pos:menu.instalment")}
          </Button>
        ) : null}
        {on("orders") && can("documents.quote") ? (
          <Button
            className={item}
            disabled={!hasLines || exchanging}
            onClick={() => ctx.open({ kind: "order" })}
          >
            {t("pos:menu.order")}
          </Button>
        ) : null}
        {/* M8 (LOY-001): the gift voucher, on the till's key map's `gift_voucher` key (Ctrl+3) */}
        {on("gift_vouchers") && can("sales.create") ? (
          <Button
            className={item}
            onClick={() => ctx.open({ kind: "giftVoucher" })}
            shortcut={ariaKey(ctx.keys.gift_voucher)}
            shortcutLabel={keyLabel(ctx.keys.gift_voucher, t)}
          >
            {t("pos:menu.giftVoucher")}
          </Button>
        ) : null}
        {/* M8: the packs' till actions (VRT-002 top-up, VRT-001 department keys, VRT-101 van) */}
        {on("flexy") ? (
          <Button
            className={item}
            onClick={() => ctx.open({ kind: "pack", view: { kind: "flexy" } })}
            shortcut={ariaKey(ctx.keys.flexy)}
            shortcutLabel={keyLabel(ctx.keys.flexy, t)}
          >
            {t("pos:menu.flexy")}
          </Button>
        ) : null}
        {on("department_keys") ? (
          <Button
            className={item}
            onClick={() => {
              ctx.toggleDepartments();
              onClose();
            }}
            shortcut={ariaKey(ctx.keys.department_keys)}
            shortcutLabel={keyLabel(ctx.keys.department_keys, t)}
          >
            {t("pos:menu.departments")}
          </Button>
        ) : null}
        {on("delivery_rounds") ? (
          <Button className={item} onClick={() => ctx.open({ kind: "pack", view: { kind: "van" } })}>
            {t("pos:menu.van")}
          </Button>
        ) : null}
        <Button
          className={item}
          onClick={() => ctx.open({ kind: "price" })}
          shortcut={ariaKey(ctx.keys.price_check)}
          shortcutLabel={keyLabel(ctx.keys.price_check, t)}
        >
          {t("pos:menu.priceCheck")}
        </Button>
        <Button
          className={item}
          onClick={() => ctx.open({ kind: "held" })}
          shortcut={ariaKey(ctx.keys.held)}
          shortcutLabel={keyLabel(ctx.keys.held, t)}
        >
          {t("pos:menu.held")}
        </Button>
        <Button
          className={item}
          onClick={() => {
            useCart.getState().repeatLast();
            onClose();
          }}
          shortcut={ariaKey(ctx.keys.repeat_last)}
          shortcutLabel={keyLabel(ctx.keys.repeat_last, t)}
        >
          {t("pos:menu.repeat")}
        </Button>
        <label htmlFor="pos-level" className="font-semibold">
          {t("pos:menu.priceLevel")}
        </label>
        <NativeSelect
          id="pos-level"
          value={priceLevelId ?? ""}
          onChange={(e) => useCart.getState().setPriceLevel(e.target.value || null)}
        >
          {(levels.data ?? [])
            .filter((l) => !l.archived)
            .map((l) => (
              <option key={l.id} value={l.isDefault ? "" : l.id}>
                {localizedName(l, lang)}
              </option>
            ))}
        </NativeSelect>
        <p className="font-semibold">{t("pos:menu.docLang")}</p>
        <LanguageToggle value={docLang} onChange={(l) => useCart.getState().setLang(l)} />
        {hasLines ? (
          confirmCancel ? (
            <Button variant="danger" className={item} onClick={() => void ctx.cancelSale().then(onClose)}>
              {t("pos:menu.confirmCancel")}
            </Button>
          ) : (
            <Button variant="danger" className={item} onClick={() => setConfirmCancel(true)}>
              {t("pos:menu.cancelSale")}
            </Button>
          )
        ) : null}
        <hr className="border-line" />
        <Button variant="outline" className={item} onClick={() => go("/sales")}>
          {t("pos:menu.history")}
        </Button>
        <Button variant="outline" className={item} onClick={() => go("/cash")}>
          {t("pos:menu.cash")}
        </Button>
        <Button variant="outline" className={item} onClick={() => go(backOfficeHome(ctx.session))}>
          {t("pos:menu.backOffice")}
        </Button>
        <Button variant="ghost" className={item} onClick={() => logout.mutate()}>
          {t("pos:menu.lock")}
        </Button>
      </div>
    </Sheet>
  );
}
