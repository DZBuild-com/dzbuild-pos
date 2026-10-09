import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { ScanInput } from "@/components/scan-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { useCart } from "@/features/pos/cart-store";
import type { PosContext } from "@/features/pos/PosDialogs";
import { isApprovalCancelled } from "@/lib/approval";
import { useOfferCart } from "@/lib/ipc/billing";
import { useFindDocument } from "@/lib/ipc/documents";
import { usePriceCart } from "@/lib/ipc/sales";
import { useErrorText } from "@/lib/use-error-text";

/**
 * SAL-046, Ctrl+D at the till: a validated quote or proforma typed or scanned by its number comes
 * into the (empty) cart like a recalled ticket, priced at today's prices by the backend; the lines
 * whose price changed since the offer are flagged, keeping the offered price being a price
 * override. The ticket is linked to the quote once it is paid.
 */
export function LoadQuoteDialog({ ctx, onClose }: { ctx: PosContext; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const find = useFindDocument();
  const offer = useOfferCart();
  const price = usePriceCart();
  const [number, setNumber] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = async (typed: string) => {
    const wanted = typed.trim();
    if (wanted === "") return;
    const cart = useCart.getState();
    if (cart.lines.length > 0 || cart.exchange) {
      setProblem(t("pos:held.busy"));
      return;
    }
    setProblem(null);
    setLoading(true);
    try {
      const quote = await find.mutateAsync(wanted);
      if (
        !quote ||
        (quote.docType !== "quote" && quote.docType !== "proforma") ||
        quote.status !== "validated"
      ) {
        setProblem(t("billing:view.notFound"));
        return;
      }
      const offered = await offer.mutateAsync(quote.id);
      // an offered price kept on a line is a price override: a manager may have to approve it
      const priced = await ctx.run(() => price.mutateAsync(offered));
      const displays = priced.cart.lines.map((l) => ({
        name: l.description,
        unitCode: l.unitCode,
        unitDecimals: l.unitCode === "pc" ? 0 : 3,
        price: l.unitPrice,
      }));
      useCart.getState().loadOffer(offered, displays, { id: quote.id, number: quote.number ?? wanted });
      if (priced.cart.lines.some((l) => l.priceChanged)) ctx.notify("info", t("billing:offer.priceChanged"));
      onClose();
    } catch (error) {
      if (!isApprovalCancelled(error)) setProblem(errorText(error));
    } finally {
      setLoading(false);
    }
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    void load(number);
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("billing:offer.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            type="submit"
            form="load-quote"
            variant="primary"
            disabled={loading || number.trim() === ""}
          >
            {t("billing:offer.load")}
          </Button>
        </>
      }
    >
      <form id="load-quote" noValidate onSubmit={submit}>
        <Field id="load-quote-number" label={t("billing:offer.number")} error={problem ?? undefined}>
          <ScanInput
            id="load-quote-number"
            dir="ltr"
            autoFocus
            value={number}
            onChange={(event) => setNumber(event.target.value)}
            onScan={(code) => {
              setNumber(code);
              void load(code);
            }}
          />
        </Field>
      </form>
    </Dialog>
  );
}
