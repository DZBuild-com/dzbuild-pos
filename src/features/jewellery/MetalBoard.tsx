import { History } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { Sheet } from "@/components/ui/sheet";
import { formatDate } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { useMetalBoard, useMetalHistory, useRepriceJewellery, useSetMetalPrice } from "@/lib/ipc/jewellery";
import type { Metal, Permission, RepriceReport } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { METALS, carats, finenessLabel } from "./fineness";

interface Grade {
  metal: Metal;
  titre: number;
}

/**
 * VRT-140: the price per gram of each metal and fineness, when and by whom it was set; "Nouveau
 * prix" puts a new one on the board (the old one stays in its history) and "Recalculer les prix
 * du catalogue" writes the day's price of the pieces sold by weight into the catalogue. On a till
 * these writes go to the main PC.
 */
export function MetalBoard() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const session = useSession();
  const board = useMetalBoard();
  const reprice = useRepriceJewellery();
  const [editing, setEditing] = useState<Grade | "new" | null>(null);
  const [history, setHistory] = useState<Grade | null>(null);
  const [repriced, setRepriced] = useState<RepriceReport | null>(null);
  if (board.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(board.error)}
      </p>
    );
  }
  // the buttons shown depend on the user: the board waits for the session too
  if (!board.data || !session.data) return <p className="text-ink-2">{t("common:status.loading")}</p>;
  const permissions = session.data.permissions;
  const can = (p: Permission) => permissions.includes(p);
  const prices = can("products.price");
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-2xl font-bold">{t("jewellery:board.title")}</h2>
        {prices ? (
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={() => setEditing("new")}>
              {t("jewellery:board.add")}
            </Button>
            <Button
              variant="primary"
              disabled={reprice.isPending}
              onClick={() => {
                setRepriced(null);
                reprice.mutate(undefined, { onSuccess: setRepriced });
              }}
            >
              {t("jewellery:board.reprice")}
            </Button>
          </div>
        ) : null}
      </div>
      {prices ? <p className="text-sm text-ink-2">{t("jewellery:board.repriceHint")}</p> : null}
      {repriced ? (
        <p role="status" className="rounded-control bg-ok-soft p-3 font-semibold text-ok">
          {t("jewellery:board.repriced", { count: repriced.changed })}
        </p>
      ) : null}
      {reprice.error ? (
        <p role="alert" className="text-danger">
          {errorText(reprice.error)}
        </p>
      ) : null}
      {board.data.length === 0 ? (
        <p className="rounded-card border border-dashed border-line p-6 text-center text-ink-2">
          {t("jewellery:board.empty")}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-card border border-line bg-surface">
          <table className="w-full text-start">
            <thead className="bg-surface-2 text-sm text-ink-2">
              <tr>
                <th scope="col" className="p-3 text-start font-semibold">
                  {t("jewellery:board.metalTitre")}
                </th>
                <th scope="col" className="p-3 text-end font-semibold">
                  {t("jewellery:board.price")}
                </th>
                <th scope="col" className="p-3 text-start font-semibold">
                  {t("jewellery:board.since")}
                </th>
                <th scope="col" className="p-3">
                  <span className="sr-only">{t("jewellery:board.actions")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {board.data.map((price) => {
                const grade = { metal: price.metal, titre: price.titrePermille };
                return (
                  <tr key={`${price.metal}-${price.titrePermille}`} className="border-t border-line">
                    <th scope="row" className="p-3 text-start font-semibold">
                      {finenessLabel(t, price.metal, price.titrePermille)}
                    </th>
                    <td className="p-3 text-end text-xl font-bold">
                      <Amount value={price.pricePerGram} />
                    </td>
                    <td className="p-3 text-ink-2">
                      {t("jewellery:board.setBy", {
                        date: formatDate(price.effectiveAt, lang, { withTime: true }),
                        name: price.userName,
                      })}
                    </td>
                    <td className="p-3">
                      <div className="flex flex-wrap justify-end gap-2">
                        {prices ? (
                          <Button variant="secondary" onClick={() => setEditing(grade)}>
                            {t("jewellery:board.new")}
                          </Button>
                        ) : null}
                        {can("products.view") ? (
                          <Button variant="ghost" onClick={() => setHistory(grade)}>
                            <History aria-hidden />
                            {t("jewellery:board.history")}
                          </Button>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {editing ? (
        <PriceDialog grade={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
      ) : null}
      {history ? <HistorySheet grade={history} onClose={() => setHistory(null)} /> : null}
    </div>
  );
}

/** A new price per gram for a metal and fineness of the board, or for a new one (`grade: null`:
 * the metal and the fineness are chosen here). */
function PriceDialog({ grade, onClose }: { grade: Grade | null; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const setPrice = useSetMetalPrice();
  const [metal, setMetal] = useState<Metal>(grade?.metal ?? "gold");
  const [titre, setTitre] = useState(String(grade?.titre ?? 750));
  const [price, setPriceText] = useState("");
  const [tried, setTried] = useState(false);
  const fineness = /^\d+$/.test(titre) ? Number(titre) : 0;
  const titreOk = fineness >= 1 && fineness <= 1000;
  const perGram = parseAmount(price);
  const priceOk = perGram !== null && perGram > 0;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTried(true);
    if (!titreOk || !priceOk) return;
    setPrice.mutate(
      { metal, titre: fineness, pricePerGram: perGram },
      {
        onSuccess: () => {
          toast.success(t("jewellery:board.saved"));
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={
        grade
          ? t("jewellery:board.newFor", { fineness: finenessLabel(t, grade.metal, grade.titre) })
          : t("jewellery:board.add")
      }
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="metal-price-form" variant="primary" disabled={setPrice.isPending}>
            {t("jewellery:board.save")}
          </Button>
        </>
      }
    >
      <form id="metal-price-form" noValidate onSubmit={submit} className="flex flex-col gap-4">
        {grade ? null : (
          <div className="grid gap-4 sm:grid-cols-2">
            <Field id="metal-price-metal" label={t("jewellery:tab.metal")}>
              <NativeSelect
                id="metal-price-metal"
                value={metal}
                onChange={(event) => {
                  const chosen = METALS.find((m) => m === event.target.value);
                  if (chosen) setMetal(chosen);
                }}
              >
                {METALS.map((m) => (
                  <option key={m} value={m}>
                    {t(`jewellery:metal.${m}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field
              id="metal-price-titre"
              label={t("jewellery:tab.titre")}
              hint={
                metal === "gold" && titreOk
                  ? t("jewellery:tab.carats", { carats: carats(fineness) })
                  : undefined
              }
              error={tried && !titreOk ? t("validation:required") : undefined}
            >
              <NumericInput id="metal-price-titre" kind="integer" value={titre} onValueChange={setTitre} />
            </Field>
          </div>
        )}
        <Field
          id="metal-price"
          label={t("jewellery:board.price")}
          error={tried && !priceOk ? t("validation:amountFormat") : undefined}
        >
          <NumericInput
            id="metal-price"
            autoFocus
            value={price}
            onValueChange={setPriceText}
            className="text-2xl font-bold"
          />
        </Field>
        {setPrice.error ? (
          <p role="alert" className="text-danger">
            {errorText(setPrice.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}

/** The prices a metal and fineness had, newest first. */
function HistorySheet({ grade, onClose }: { grade: Grade; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const history = useMetalHistory(grade.metal, grade.titre);
  return (
    <Sheet
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("jewellery:board.historyFor", { fineness: finenessLabel(t, grade.metal, grade.titre) })}
    >
      {history.error ? (
        <p role="alert" className="text-danger">
          {errorText(history.error)}
        </p>
      ) : (
        <ul className="flex flex-col">
          {(history.data ?? []).map((price) => (
            <li
              key={`${price.effectiveAt}-${price.pricePerGram}`}
              className="flex flex-wrap items-center justify-between gap-2 border-b border-line py-3"
            >
              <span className="text-ink-2">
                {t("jewellery:board.setBy", {
                  date: formatDate(price.effectiveAt, lang, { withTime: true }),
                  name: price.userName,
                })}
              </span>
              <Amount value={price.pricePerGram} className="font-bold" />
            </li>
          ))}
        </ul>
      )}
    </Sheet>
  );
}
