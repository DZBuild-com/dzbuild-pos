import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { formatAmountInput, formatDZD } from "@/lib/format";
import { useJewelPiece, useSetJewelPiece } from "@/lib/ipc/jewellery";
import type { Metal, Piece, PieceInput, Product } from "@/lib/ipc/types";
import { formatQty, parseAmount, parseQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { METALS, carats } from "./fineness";

/** The product form's "Bijou" tab (VRT-140, VRT-143), shown while the jewellery module is on:
 * the piece's metal, fineness, weight, making charge, stones and hallmark, and whether it is
 * priced by weight; its price at today's board comes from the backend. */
export function JewelTab({ product, editable }: { product: Product; editable: boolean }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const piece = useJewelPiece(product.id);
  if (piece.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(piece.error)}
      </p>
    );
  }
  if (piece.data === undefined) return <p className="text-ink-2">{t("common:status.loading")}</p>;
  return <JewelForm key={product.id} productId={product.id} piece={piece.data} editable={editable} />;
}

const text = (value: string) => (value.trim() === "" ? null : value.trim());

function JewelForm({
  productId,
  piece,
  editable,
}: {
  productId: string;
  piece: Piece | null;
  editable: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const save = useSetJewelPiece();
  const [metal, setMetal] = useState<Metal>(piece?.metal ?? "gold");
  const [titre, setTitre] = useState(String(piece?.titrePermille ?? 750));
  const [weight, setWeight] = useState(piece ? formatQty(piece.weightMg, lang) : "");
  const [facon, setFacon] = useState(piece ? formatAmountInput(piece.facon, lang) : "");
  const [stones, setStones] = useState(piece?.stones ?? "");
  const [hallmark, setHallmark] = useState(piece?.hallmark ?? "");
  const [byWeight, setByWeight] = useState(piece?.pricedByWeight ?? true);
  const [tried, setTried] = useState(false);
  const fineness = /^\d+$/.test(titre) ? Number(titre) : 0;
  const titreOk = fineness >= 1 && fineness <= 1000;
  const weightMg = parseQty(weight, 3);
  const making = facon.trim() === "" ? 0 : parseAmount(facon);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTried(true);
    if (!titreOk || weightMg === null || making === null) return;
    const input: PieceInput = {
      metal,
      titrePermille: fineness,
      weightMg,
      facon: making,
      stones: text(stones),
      hallmark: text(hallmark),
      pricedByWeight: byWeight,
    };
    save.mutate({ productId, input }, { onSuccess: () => toast.success(t("jewellery:tab.saved")) });
  };
  return (
    <form noValidate onSubmit={submit} className="flex flex-col gap-4">
      {piece === null ? <p className="text-ink-2">{t("jewellery:tab.notPiece")}</p> : null}
      {piece?.pricedByWeight ? (
        <p role="status" className="rounded-card bg-surface-2 p-4 text-xl font-bold">
          {piece.livePrice !== null
            ? t("jewellery:tab.live", { amount: formatDZD(piece.livePrice, lang) })
            : t("jewellery:tab.noPrice")}
        </p>
      ) : null}
      <fieldset disabled={!editable} className="grid gap-4 sm:grid-cols-3">
        <Field id="jewel-metal" label={t("jewellery:tab.metal")}>
          <NativeSelect
            id="jewel-metal"
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
          id="jewel-titre"
          label={t("jewellery:tab.titre")}
          hint={
            metal === "gold" && titreOk ? t("jewellery:tab.carats", { carats: carats(fineness) }) : undefined
          }
          error={tried && !titreOk ? t("validation:required") : undefined}
        >
          <NumericInput id="jewel-titre" kind="integer" value={titre} onValueChange={setTitre} />
        </Field>
        <Field
          id="jewel-weight"
          label={t("jewellery:tab.weight")}
          error={tried && weightMg === null ? t("validation:required") : undefined}
        >
          <NumericInput id="jewel-weight" kind="qty" value={weight} onValueChange={setWeight} />
        </Field>
        <Field
          id="jewel-facon"
          label={t("jewellery:tab.facon")}
          error={tried && making === null ? t("validation:amountFormat") : undefined}
        >
          <NumericInput id="jewel-facon" value={facon} onValueChange={setFacon} />
        </Field>
        <Field id="jewel-stones" label={t("jewellery:tab.stones")}>
          <Input
            id="jewel-stones"
            dir="auto"
            maxLength={200}
            value={stones}
            onChange={(event) => setStones(event.target.value)}
          />
        </Field>
        <Field id="jewel-hallmark" label={t("jewellery:tab.hallmark")}>
          <Input
            id="jewel-hallmark"
            dir="auto"
            maxLength={40}
            value={hallmark}
            onChange={(event) => setHallmark(event.target.value)}
          />
        </Field>
        <label className="flex min-h-11 items-center gap-3 font-semibold sm:col-span-3">
          <Checkbox checked={byWeight} onCheckedChange={(on) => setByWeight(on === true)} />
          <span className="flex flex-col">
            {t("jewellery:tab.byWeight")}
            <span className="text-sm font-normal text-ink-2">{t("jewellery:tab.byWeightHint")}</span>
          </span>
        </label>
      </fieldset>
      {save.error ? (
        <p role="alert" className="text-danger">
          {errorText(save.error)}
        </p>
      ) : null}
      {editable ? (
        <Button type="submit" variant="primary" className="self-start" disabled={save.isPending}>
          {t("jewellery:tab.save")}
        </Button>
      ) : null}
    </form>
  );
}
