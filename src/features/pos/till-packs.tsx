/**
 * The till's pack extensions (Task 24): a chunk of its own, read through `pack-slots.ts`
 * only when a pack module needs it, so the /pos route stays within its budget. The entry hands it
 * the cart and the extras store through the till kit (`till-kit.ts`) instead of it importing them.
 */
import { useQueryClient } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { useEffect, useState } from "react";
import type { Lang } from "@/lib/i18n";
import { customerOfCard, describeVoucher } from "@/lib/ipc/till-packs";
import type { Module } from "@/lib/ipc/types";
import { BuyerDialog } from "./BuyerDialog";
import { customerIdentified, lineFacts, tracksSerials } from "./checkout-facts";
import { type CheckoutLine, type CheckoutStep, checkoutSteps } from "./checkout-steps";
import { FlexyDialog } from "./FlexyDialog";
import type { PackView, PosContext } from "./PosDialogs";
import { PrescriptionDialog } from "./PrescriptionDialog";
import { SerialDialog } from "./SerialDialog";
import { useTillKit } from "./till-kit";
import { VanPicker } from "./VanBanner";
import { VariantPicker } from "./VariantPicker";
import { voucherText } from "./VoucherTender";

export { DepartmentKeysPanel } from "./DepartmentKeysPanel";
export { PaymentTiles, PointsLine } from "./PaymentPacks";
export { ReturnSerials } from "./SerialDialog";
export { LineNotices } from "./SubstitutionNotice";
export { VanBanner } from "./VanBanner";

export { tracksSerials };

/**
 * A code the till did not find as a product (M8): a loyalty card sets the ticket's customer
 * (LOY-002), a voucher or a deposit says it is spent at the payment. `null` for anything else.
 */
export async function readPackCode(
  code: string,
  modules: readonly Module[],
  t: TFunction,
  lang: Lang,
): Promise<{ customerId: string | null; notice: string } | null> {
  if (modules.includes("loyalty")) {
    const customer = await customerOfCard(code).catch(() => null);
    if (customer) return { customerId: customer.id, notice: t("packs:till.card", { name: customer.name }) };
  }
  const info = await describeVoucher(code).catch(() => null);
  if (info && info.target.kind !== "store_credit" && info.balance > 0) {
    return {
      customerId: null,
      notice: t("packs:till.voucher.atPayment", { voucher: voucherText(info, t, lang) }),
    };
  }
  return null;
}

/** "Encaisser" with a pack module on: the steps the sale needs, one dialog each, then the payment
 * dialog. */
function Checkout({ ctx, onClose }: { ctx: PosContext; onClose: () => void }) {
  const client = useQueryClient();
  const { useCart, useExtras } = useTillKit();
  const [steps, setSteps] = useState<CheckoutStep[] | null>(null);
  const { modules, checkoutDone } = ctx;

  useEffect(() => {
    let live = true;
    const cart = useCart.getState();
    const extras = useExtras.getState();
    // a fact that cannot be read asks nothing: the backend checks the sale again at "Valider"
    const facts = (index: number): Promise<CheckoutLine> => {
      const draft = cart.lines[index];
      return lineFacts(client, modules, {
        productId: draft?.line.productId ?? null,
        name: draft?.display.name ?? "",
        qty: draft?.line.qty ?? 0,
        serials: extras.serials[index]?.length ?? 0,
      });
    };
    void Promise.all([
      Promise.all(cart.lines.map((_, index) => facts(index))),
      customerIdentified(client, modules, cart.customerId),
    ])
      .then(([lines, customerIdentified]) =>
        checkoutSteps({
          lines,
          prescribed: extras.prescriptions.map((p) => p.line),
          hasBuyer: extras.buyer !== null,
          customerIdentified,
        }),
      )
      .catch(() => [])
      .then((found) => {
        if (live) setSteps(found);
      });
    return () => {
      live = false;
    };
  }, [client, modules, useCart, useExtras]);

  useEffect(() => {
    if (steps?.length === 0) checkoutDone();
  }, [steps, checkoutDone]);

  const step = steps?.[0];
  if (!step) return null;
  const next = () => setSteps((all) => (all ? all.slice(1) : all));
  const extras = useExtras.getState();
  switch (step.kind) {
    case "serials":
      return (
        <SerialDialog
          key={`serials-${step.line}`}
          productId={step.productId}
          name={step.name}
          count={step.count}
          initial={extras.serials[step.line] ?? []}
          onDone={(list) => {
            useExtras.getState().setSerials(step.line, list);
            next();
          }}
          onClose={onClose}
        />
      );
    case "prescriptions":
      return (
        <PrescriptionDialog
          lines={step.lines}
          initial={extras.prescriptions}
          onDone={(given) => {
            const kept = useExtras
              .getState()
              .prescriptions.filter((p) => !given.some((g) => g.line === p.line));
            useExtras.getState().setPrescriptions([...kept, ...given]);
            next();
          }}
          onClose={onClose}
        />
      );
    case "buyer":
      return (
        <BuyerDialog
          initial={extras.buyer}
          onDone={(buyer) => {
            useExtras.getState().setBuyer(buyer);
            next();
          }}
          onClose={onClose}
        />
      );
  }
}

/** Ctrl+4: the serials of the selected line, when its product is sold by serial. */
function LineSerials({ line, onClose }: { line: number; onClose: () => void }) {
  const client = useQueryClient();
  const { useCart, useExtras } = useTillKit();
  const draft = useCart((s) => s.lines[line]);
  const productId = draft?.line.productId ?? null;
  const [tracked, setTracked] = useState<boolean | null>(null);
  useEffect(() => {
    let live = true;
    if (!productId) return undefined;
    void tracksSerials(client, productId)
      .catch(() => false)
      .then((yes) => {
        if (live) setTracked(yes);
      });
    return () => {
      live = false;
    };
  }, [client, productId]);
  const pieces = Math.floor((draft?.line.qty ?? 0) / 1_000);
  useEffect(() => {
    if (!productId || tracked === false || pieces === 0) onClose();
  }, [productId, tracked, pieces, onClose]);
  if (!productId || !draft || !tracked || pieces === 0) return null;
  return (
    <SerialDialog
      productId={productId}
      name={draft.display.name}
      count={pieces}
      initial={useExtras.getState().serials[line] ?? []}
      onDone={(list) => {
        useExtras.getState().setSerials(line, list);
        onClose();
      }}
      onClose={onClose}
    />
  );
}

/** A dialog of the till's pack extensions (`PosDialogs`' `pack` kind). */
export function PackDialog({ view, ctx, onClose }: { view: PackView; ctx: PosContext; onClose: () => void }) {
  switch (view.kind) {
    case "flexy":
      return <FlexyDialog onClose={onClose} />;
    case "serial":
      return <LineSerials line={view.line} onClose={onClose} />;
    case "variants":
      return (
        <VariantPicker
          parent={view.parent}
          onPick={(variant) => {
            onClose();
            ctx.addProduct(variant, 1_000, 0);
          }}
          onClose={onClose}
        />
      );
    case "van":
      return <VanPicker onClose={onClose} />;
    case "checkout":
      return <Checkout ctx={ctx} onClose={onClose} />;
  }
}
