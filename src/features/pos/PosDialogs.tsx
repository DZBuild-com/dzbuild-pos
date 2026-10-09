import { type ReactNode, Suspense, lazy } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { Hotkey } from "@/lib/hotkeys";
import type {
  ExchangeDone,
  Module,
  PaymentPreview,
  ProductHit,
  ReturnDone,
  SaleDone,
  Session,
} from "@/lib/ipc/types";
import type { TillAction } from "@/lib/key-map";
import { UnprintedSheet } from "@/app/PrintFailures";
import { OpenSessionDialog } from "@/features/cash/OpenSessionDialog";
import { LoadQuoteDialog } from "@/features/documents/LoadQuoteDialog";
import { loadBackOffice } from "@/lib/i18n";
import { useLinkSale } from "@/lib/ipc/billing";
import { toCart, useCart } from "./cart-store";
import { useExtras } from "./extras-store";
import { CustomerPicker } from "./CustomerPicker";
import { DiscountDialog } from "./DiscountDialog";
import { HeldTicketsSheet } from "./HeldTicketsSheet";
import { HelpDialog } from "./HelpDialog";
import { HoldDialog } from "./HoldDialog";
import { LineEditDialog } from "./LineEditDialog";
import { NoSaleDialog } from "./NoSaleDialog";
import { OpenItemDialog } from "./OpenItemDialog";
import { PackDialog } from "./pack-slots";
import { PaymentDialog } from "./PaymentDialog";
import { PosMenu } from "./PosMenu";
import { PriceCheckDialog } from "./PriceCheckDialog";
import { SaleDoneDialog } from "./SaleDoneDialog";
import type { WeighedAdd } from "./ScalePanel";
import type { Weighing } from "./use-customer-display";

// M8: a repair handed back at the till (`/services` → "Remettre au client") is a pack chunk of its
// own with the back office's texts, never in the till's entry; the till hands it its
// payment dialog and cart rather than the chunk importing them (which would split the entry).
const ServicePickup = lazy(() =>
  Promise.all([import("@/features/services/ServicePickup"), loadBackOffice()]).then(([m]) => ({
    default: m.ServicePickup,
  })),
);

// M8 (Task 27): an instalment contract from the till's cart, the cart put on order, and an order
// served at the till: pack chunks too, mounted here (the till hands them its cart, its preview's
// total and its payment dialog; they import none of the till's modules).
const InstalmentDialog = lazy(() =>
  Promise.all([import("@/features/instalments/InstalmentDialog"), loadBackOffice()]).then(([m]) => ({
    default: m.InstalmentDialog,
  })),
);
const OrderDialog = lazy(() =>
  Promise.all([import("@/features/customer-orders/OrderDialog"), loadBackOffice()]).then(([m]) => ({
    default: m.OrderDialog,
  })),
);
const OrderFulfil = lazy(() =>
  Promise.all([import("@/features/customer-orders/OrderFulfil"), loadBackOffice()]).then(([m]) => ({
    default: m.OrderFulfil,
  })),
);

const TILL_PIECES = { PaymentDialog, useCart, PackDialog };

// The /pos budget (NFR-010): the return dialog (F8) and the scale panel (F11) are chunks of
// their own, read the first time they open; their words are the till's (already loaded).
const ReturnDialog = lazy(() => import("./ReturnDialog").then((m) => ({ default: m.ReturnDialog })));
const ScalePanel = lazy(() => import("./ScalePanel").then((m) => ({ default: m.ScalePanel })));

/** Empties the till once its cart became a signed contract or a recorded order. */
const clearTill = () => useCart.getState().clear(useCart.getState().lang);

// M8 (LOY-001): a gift voucher sold from the menu. Its dialog is a chunk of its own, read with the
// back office's texts (the `loyalty` namespace), so the till's first paint does not carry it.
// The till's key map opens it too (`gift_voucher`, Ctrl+3 by default, UX-017).
const SellVoucherDialog = lazy(() =>
  Promise.all([import("@/features/loyalty/SellVoucherDialog"), loadBackOffice()]).then(([module]) => ({
    default: module.SellVoucherDialog,
  })),
);

export type Done =
  | { kind: "sale"; result: SaleDone }
  | { kind: "exchange"; result: ExchangeDone }
  | { kind: "return"; result: ReturnDone };

export type PosDialog =
  | { kind: "customer" }
  | { kind: "edit"; index: number }
  | { kind: "discount" }
  | { kind: "hold" }
  | { kind: "held" }
  | { kind: "price" }
  | { kind: "drawer" }
  | { kind: "openItem" }
  | { kind: "menu" }
  | { kind: "help" }
  | { kind: "openSession" }
  | { kind: "pay" }
  | { kind: "return"; number?: string }
  | { kind: "unprinted" }
  | { kind: "quote" }
  | { kind: "scale" }
  /** A ready repair or service ticket: its lines and its deposit at the till (`service_pickup`). */
  | { kind: "pickup"; ticketId: string }
  /** An instalment contract from the cart (`instalments_sign`). */
  | { kind: "instalment" }
  /** The cart put on order or layaway (`orders_create`). */
  | { kind: "order" }
  /** A customer order served at the till: its lines and its deposit (`orders_fulfil`). */
  | { kind: "fulfil"; orderId: string }
  /** LOY-001 (Ctrl+3): a gift voucher sold from the till. */
  | { kind: "giftVoucher" }
  | { kind: "pack"; view: PackView }
  | { kind: "done"; done: Done };

/** M8: a dialog of the till's pack extensions (`till-packs.tsx`, a lazy chunk). */
export type PackView =
  /** VRT-002 (Ctrl+2): a top-up line. */
  | { kind: "flexy" }
  /** SAL-025 (Ctrl+4): the serials of cart line `line`. */
  | { kind: "serial"; line: number }
  /** SAL-023: the size × colour grid of a variant model. */
  | { kind: "variants"; parent: ProductHit }
  /** VRT-101: the van of a round the till sells from. */
  | { kind: "van" }
  /** "Encaisser": the serials, prescriptions and buyer the sale needs, then the payment. */
  | { kind: "checkout" };

/** A catalogue product as the till adds it (a scan, a tile, a variant of the grid). */
export interface TillProduct {
  id: string;
  kind: string;
  unitCode: string;
  sellPrice: number;
  nameAr: string;
  nameFr: string;
  nameEn: string;
}

export interface PosContext {
  session: Session;
  preview: PaymentPreview | undefined;
  run: <T>(action: () => Promise<T>) => Promise<T>;
  voidLine: (index: number) => Promise<void>;
  /** Lowers line `index` to `qty` milli-units, journaled as a partial void (`sales.void_line`,
   * a manager's approval when needed). Resolves `false` when refused or cancelled. */
  reduceQty: (index: number, qty: number) => Promise<boolean>;
  cancelSale: () => Promise<void>;
  /** Takes a held ticket back into the (empty) cart; refused with a notice while it has lines.
   * Rejects with the backend's error. */
  recallHeld: (id: string) => Promise<void>;
  notify: (tone: "info" | "error", text: string) => void;
  open: (dialog: PosDialog) => void;
  /** The category chosen on the sale screen: F11 offers its weighed products. */
  categoryId: string | null;
  /** F11: a stable weight of a product, added as a line of its own. */
  addWeighed: (add: WeighedAdd) => void;
  /** F11: what the customer display shows meanwhile (`null` once the panel closes). */
  onWeighing: (weighing: Weighing | null) => void;
  /** M8: the pack modules on, the till's keys (UX-017) and the actions bound to them. */
  modules: readonly Module[];
  keys: Record<TillAction, Hotkey>;
  actions: readonly TillAction[];
  /** Adds a catalogue product as a scan does (a variant picked in the grid, a generic). */
  addProduct: (product: TillProduct, qty: number, unitDecimals: number) => void;
  /** The checkout's pack steps are done: the payment dialog opens. */
  checkoutDone: () => void;
  /** Department keys in place of the product grid, or back (Ctrl+5). */
  toggleDepartments: () => void;
}

export function PosDialogs({
  dialog,
  onClose,
  ctx,
}: {
  dialog: PosDialog | null;
  onClose: () => void;
  ctx: PosContext;
}): ReactNode {
  const { t } = useTranslation();
  const linkSale = useLinkSale();
  switch (dialog?.kind) {
    case "customer":
      return <CustomerPicker ctx={ctx} onClose={onClose} />;
    case "edit":
      return <LineEditDialog index={dialog.index} ctx={ctx} onClose={onClose} />;
    case "discount":
      return <DiscountDialog onClose={onClose} />;
    case "hold":
      return <HoldDialog ctx={ctx} onClose={onClose} />;
    case "held":
      return <HeldTicketsSheet ctx={ctx} onClose={onClose} />;
    case "price":
      return <PriceCheckDialog onClose={onClose} />;
    case "drawer":
      return <NoSaleDialog ctx={ctx} onClose={onClose} />;
    case "openItem":
      return <OpenItemDialog onClose={onClose} />;
    case "menu":
      return <PosMenu ctx={ctx} onClose={onClose} />;
    case "help":
      return <HelpDialog keys={ctx.keys} actions={ctx.actions} onClose={onClose} />;
    case "openSession":
      return <OpenSessionDialog open onOpenChange={(open) => (open ? undefined : onClose())} />;
    case "pay":
      return (
        <PaymentDialog
          onClose={onClose}
          onDone={(done) => {
            // the ticket is committed: its lines and its return leave the cart now, not on
            // "Nouvelle vente", so a lock or a logout on the change screen cannot bring them
            // back to be sold or refunded twice (the done dialog reads only `done.result`)
            const state = useCart.getState();
            const quote = state.fromQuote;
            state.clear(state.lang);
            // SAL-046: the ticket is recorded as made from the quote; the sale stands if it fails
            const ticket =
              done.kind === "sale"
                ? done.result.sale.document
                : done.kind === "exchange"
                  ? done.result.exchange.sale.document
                  : null;
            if (quote && ticket) {
              linkSale.mutate(
                { quoteId: quote.id, ticketId: ticket.id },
                { onError: () => toast.warning(t("billing:offer.linkFailed")) },
              );
            }
            ctx.open({ kind: "done", done });
          }}
        />
      );
    case "return":
      return (
        <Suspense fallback={null}>
          <ReturnDialog number={dialog.number} ctx={ctx} onClose={onClose} />
        </Suspense>
      );
    case "done":
      return <SaleDoneDialog done={dialog.done} onNewSale={onClose} />;
    case "unprinted":
      return <UnprintedSheet onClose={onClose} />;
    case "quote":
      return <LoadQuoteDialog ctx={ctx} onClose={onClose} />;
    case "scale":
      return (
        <Suspense fallback={null}>
          <ScalePanel
            categoryId={ctx.categoryId}
            onAdd={ctx.addWeighed}
            onWeighing={ctx.onWeighing}
            onClose={onClose}
          />
        </Suspense>
      );
    case "pickup":
      return (
        <Suspense fallback={null}>
          <ServicePickup ticketId={dialog.ticketId} ctx={ctx} till={TILL_PIECES} onClose={onClose} />
        </Suspense>
      );
    case "instalment":
      return (
        <Suspense fallback={null}>
          <InstalmentDialog
            cart={toCart(useCart.getState())}
            extras={useExtras.getState().toExtras()}
            onSigned={clearTill}
            onClose={onClose}
          />
        </Suspense>
      );
    case "order":
      return (
        <Suspense fallback={null}>
          <OrderDialog
            cart={toCart(useCart.getState())}
            total={ctx.preview?.settlement.totalTtc ?? null}
            onCreated={clearTill}
            onClose={onClose}
          />
        </Suspense>
      );
    case "fulfil":
      return (
        <Suspense fallback={null}>
          <OrderFulfil orderId={dialog.orderId} ctx={ctx} till={TILL_PIECES} onClose={onClose} />
        </Suspense>
      );
    case "giftVoucher":
      return (
        <Suspense fallback={null}>
          <SellVoucherDialog onClose={onClose} />
        </Suspense>
      );
    case "pack":
      return (
        <Suspense fallback={null}>
          <PackDialog view={dialog.view} ctx={ctx} onClose={onClose} />
        </Suspense>
      );
    default:
      return null;
  }
}
