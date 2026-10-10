import { useQueryClient } from "@tanstack/react-query";
import { Lock, Menu, Percent, Pause, Scale, Undo2, User, Wallet, X } from "lucide-react";
import { type PointerEvent, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { EmptyState } from "@/components/empty-state";
import { useApproval } from "@/components/manager-approval";
import { Button } from "@/components/ui/button";
import { isApprovalCancelled } from "@/lib/approval";
import { useCurrentSession } from "@/lib/ipc/cash";
import { useUnits } from "@/lib/ipc/catalog";
import { useCompany } from "@/lib/ipc/company";
import { useSerialScans } from "@/lib/ipc/devices";
import { useHardwareConfig, useScannerOptions } from "@/lib/ipc/hardware";
import { useSearchHost, useSyncNow, useTillLink } from "@/lib/ipc/lan";
import { usePackModules } from "@/lib/ipc/pack-state";
import { useFetchCustomer } from "@/lib/ipc/customers";
import { useFindByCode } from "@/lib/ipc/products";
import { useHeldCarts, usePreviewSale, useRecallCart, useRecordCartEvent } from "@/lib/ipc/sales";
import type { Module, ProductHit, ScanHit, Session } from "@/lib/ipc/types";
import { formatAmountInput, formatDZD } from "@/lib/format";
import { type Hotkey, useCtrlHotkey, useHotkeys } from "@/lib/hotkeys";
import { DEFAULT_KEYS, type TillAction, ariaKey, keyLabel, useKeyMap } from "@/lib/key-map";
import { localizedName } from "@/lib/names";
import { useScanner } from "@/lib/scanner";
import { BackupAlerts } from "@/app/BackupAlerts";
import { DemoRibbon } from "@/app/DemoRibbon";
import { LanBanner } from "@/app/LanBanner";
import { backOfficeHome } from "@/app/nav";
import { PrinterDot } from "@/app/PrintFailures";
import { SyncPill } from "@/app/SyncPill";
import { beep } from "@/lib/sounds";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { CartPanel } from "./CartPanel";
import { toCart, useCart } from "./cart-store";
import { CustomerStrip } from "./CustomerPicker";
import { useExtras } from "./extras-store";
import { receiptByDefault, useCommitDone, useFinishSale } from "./finish-sale";
import { loadRecalled } from "./HeldTicketsSheet";
import { DepartmentKeysPanel, LineNotices, VanBanner, loadTillPacks } from "./pack-slots";
import { type Tender, exactAmount, newTender, toInputs } from "./payment-model";
import { type PosContext, type PosDialog, PosDialogs, type TillProduct } from "./PosDialogs";
import { ProductGrid } from "./ProductGrid";
import type { WeighedAdd } from "./ScalePanel";
import { ScanField } from "./ScanField";
import { useTillPayment } from "./till-payment";
import { unitDecimalsOf } from "./unit-decimals";
import { useCartPreview, usePricedLines } from "./use-cart-preview";
import { useCustomerDisplayFeed, type Weighing } from "./use-customer-display";

interface Notice {
  tone: "info" | "error";
  text: string;
}

/** The modules whose sale needs something asked before the payment (serials, prescriptions, the
 * jeweller register's buyer). */
const CHECKOUT_MODULES: Module[] = ["serials", "pharmacy", "jewellery"];
/** The modules whose vouchers or deposits a code read at the till may be. */
const VOUCHER_MODULES: Module[] = ["gift_vouchers", "loyalty", "orders", "repairs"];

/** The department keys stay in place of the product grid on this till once chosen (VRT-001). */
const DEPARTMENTS_KEY = "dzpos.till.departments";

function rememberedDepartments(): boolean {
  try {
    return window.localStorage.getItem(DEPARTMENTS_KEY) === "1";
  } catch {
    return false;
  }
}

function rememberDepartments(on: boolean): void {
  try {
    window.localStorage.setItem(DEPARTMENTS_KEY, on ? "1" : "0");
  } catch {
    // a blocked storage only forgets the choice at the next start
  }
}

/** `/pos` (spec §10.5): full window, keyboard-first and barcode-first (UX-010, UX-016, SAL-012).
 * `withTables`: the restaurant's floor is one tap away (UX-020, the "mixed" till mode). */
export function SaleScreen({ session, withTables = false }: { session: Session; withTables?: boolean }) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const location = useLocation();
  const errorText = useErrorText();
  const company = useCompany();
  const hardware = useHardwareConfig();
  const scanner = useScannerOptions();
  const till = useCurrentSession();
  const can = (p: Session["permissions"][number]) => session.permissions.includes(p);
  const held = useHeldCarts(can("sales.hold"));
  const findByCode = useFindByCode();
  // on a till, a code unknown here may be a product the main PC created a moment ago
  const lan = useTillLink();
  const searchHost = useSearchHost();
  const syncNow = useSyncNow();
  const recordEvent = useRecordCartEvent();
  const recall = useRecallCart();
  const approval = useApproval();
  const preview = useCartPreview();
  const units = useUnits().data;
  // Settings › Payments: how this till takes payment (asked, or one method at once)
  const tillPayment = useTillPayment();
  const finishSale = useFinishSale(approval.run);
  const previewSale = usePreviewSale().mutateAsync;
  const fetchCustomer = useFetchCustomer();
  const quickPaying = useRef(false);
  const [quickBusy, setQuickBusy] = useState(false);
  const lines = useCart((s) => s.lines);
  const priced = usePricedLines(lines, preview);
  const selected = useCart((s) => s.selected);
  const multiplier = useCart((s) => s.multiplier);
  const lastScan = useCart((s) => s.lastScan);
  const exchange = useCart((s) => s.exchange);
  const fromQuote = useCart((s) => s.fromQuote);
  // M8: the pack modules on, the till's keys (UX-017) and the van it sells from (VRT-101)
  const queryClient = useQueryClient();
  const modules = usePackModules();
  const keys = useKeyMap();
  const van = useExtras((s) => s.van);
  const [departmentsOn, setDepartmentsOn] = useState(rememberedDepartments);
  const departments = departmentsOn && modules.includes("department_keys");
  // the sales history sends a ticket to take items back from (RET-002); the repair board, a ready
  // ticket to hand back (M8, `{ pickup: ticketId }`); an order's page, the order to serve (M8,
  // `{ fulfil: orderId }`)
  const routed = location.state as { returnNumber?: string; pickup?: string; fulfil?: string } | null;
  const returnNumber = routed?.returnNumber;
  const pickup = routed?.pickup;
  const fulfil = routed?.fulfil;
  const [dialog, setDialog] = useState<PosDialog | null>(() =>
    returnNumber
      ? { kind: "return", number: returnNumber }
      : pickup
        ? { kind: "pickup", ticketId: pickup }
        : fulfil
          ? { kind: "fulfil", orderId: fulfil }
          : null,
  );
  const [notice, setNotice] = useState<Notice | null>(null);
  const [text, setText] = useState("");
  const [category, setCategory] = useState<string | null>(null);
  // F11's weight, shown on the customer display while the scale panel is open (VRT-010)
  const [weighing, setWeighing] = useState<Weighing | null>(null);
  const scanRef = useRef<HTMLInputElement>(null);
  const docLang = company.data?.defaultDocLang ?? session.lang;
  const empty = lines.length === 0;

  // an empty cart (a new sale) takes the shop's document language again, whatever the last
  // receipt was printed in
  useEffect(() => {
    if (empty) useCart.getState().setLang(docLang);
  }, [docLang, empty]);

  // focus returns to the scan field whenever no dialog holds it (UX-010)
  useEffect(() => {
    if (dialog === null && !approval.pending) scanRef.current?.focus();
  }, [dialog, approval.pending]);

  // the ticket sent by the sales history (or the repair board, or an order's page) is consumed: the
  // screen mounting again on this history entry (after a lock, say) does not open it a second time
  useEffect(() => {
    if (returnNumber || pickup || fulfil) void navigate(location.pathname, { replace: true, state: null });
  }, [returnNumber, pickup, fulfil, location.pathname, navigate]);

  const notify = useCallback(
    (tone: Notice["tone"], message: string) => setNotice({ tone, text: message }),
    [],
  );

  // SAL-025: a product sold by serial asks one serial per piece as soon as it is rung up
  const askSerials = useCallback(
    async (productId: string) => {
      const line = useCart.getState().selected;
      const packs = await loadTillPacks();
      if (line !== null && (await packs.tracksSerials(queryClient, productId))) {
        setDialog((current) => current ?? { kind: "pack", view: { kind: "serial", line } });
      }
    },
    [queryClient],
  );

  const addProduct = useCallback(
    (product: TillProduct, qty: number, unitDecimals: number, separate = false) => {
      if (product.kind === "variant_parent") {
        // SAL-023: the size × colour grid of the model, with the variant grid module
        if (modules.includes("variant_grid")) {
          setDialog({ kind: "pack", view: { kind: "variants", parent: product as ProductHit } });
          return;
        }
        beep("error");
        notify("error", t("pos:scan.variantParent"));
        return;
      }
      useCart.getState().addProduct({
        productId: product.id,
        qty,
        display: {
          name: localizedName(product, lang),
          unitCode: product.unitCode,
          unitDecimals,
          price: product.sellPrice,
          latinName: product.nameFr.trim() || product.nameEn.trim(),
        },
        separate,
      });
      beep("ok");
      setNotice(null);
      if (modules.includes("serials")) void askSerials(product.id).catch(() => undefined);
    },
    [askSerials, lang, modules, notify, t],
  );

  // A scale label (spec §8) is a package of its own: its line never merges with another. A price
  // label whose weight, to the gram, costs a few centimes more or less at the till says so (HW-030).
  const addHit = useCallback(
    (hit: ScanHit) => {
      addProduct(hit.product, hit.packQty, unitDecimalsOf(hit.product, units), hit.scale !== null);
      if (hit.scale && hit.scale.difference !== 0) {
        notify(
          "info",
          t("pos:scan.scaleDifference", { amount: formatDZD(Math.abs(hit.scale.difference), lang) }),
        );
      }
    },
    [addProduct, lang, notify, t, units],
  );
  const addWeighed = useCallback(
    ({ product, reading }: WeighedAdd) => {
      addProduct(product, reading.net, 3, true);
      setDialog(null);
    },
    [addProduct],
  );
  const addTile = useCallback(
    (hit: ProductHit) => addProduct(hit, 1_000, unitDecimalsOf(hit, units)),
    [addProduct, units],
  );
  // a product picked from the search results or the grid: whatever was typed is done with
  const pickProduct = useCallback(
    (hit: ProductHit) => {
      setText("");
      addTile(hit);
    },
    [addTile],
  );

  const lookup = useCallback(
    async (code: string) => {
      setText("");
      try {
        const hit = await findByCode.mutateAsync(code);
        // M8: a loyalty card sets the ticket's customer; a voucher is spent at the payment
        const packCode =
          !hit && modules.some((m) => VOUCHER_MODULES.includes(m))
            ? await (await loadTillPacks()).readPackCode(code, modules, t, lang)
            : null;
        if (hit) {
          addHit(hit);
        } else if (packCode) {
          if (packCode.customerId) useCart.getState().setCustomer(packCode.customerId);
          beep("ok");
          notify("info", packCode.notice);
        } else if (lan.isTill && lan.online) {
          const hits = await searchHost.mutateAsync({
            text: code,
            limit: 5,
            categoryId: null,
            includeArchived: false,
          });
          if (hits.length > 0) {
            notify("info", t("sync:lan.shell.foundOnHost"));
            syncNow.mutate();
          } else {
            beep("error");
            notify("error", t("sync:lan.shell.notFoundAnywhere"));
          }
        } else {
          beep("error");
          notify("error", t("pos:scan.notFound", { code }));
        }
      } catch (error) {
        beep("error");
        notify("error", errorText(error));
      }
    },
    [addHit, errorText, findByCode, lan.isTill, lan.online, lang, modules, notify, searchHost, syncNow, t],
  );

  // The scanner is read while a dialog is open too (HW-022): its Enter must never validate a
  // payment nor type a barcode into a card reference. After a sale, a scan starts the next one;
  // elsewhere the cashier is told to close the dialog first. The return and price-check dialogs
  // read scans in their own fields (`ScanInput`).
  const onScan = (code: string) => {
    if (approval.pending) return;
    // the "one method" checkout is finishing the cart on screen: a code read now would be lost
    if (quickPaying.current) {
      beep("error");
      return;
    }
    if (dialog === null) {
      void lookup(code);
    } else if (dialog.kind === "done") {
      setDialog(null);
      void lookup(code);
    } else {
      beep("error");
      toast.warning(t("pos:scan.closeDialog"));
    }
  };
  const ownScanField = dialog?.kind === "return" || dialog?.kind === "price" || dialog?.kind === "quote";
  useScanner(onScan, { ...scanner, enabled: till.data !== null && !ownScanField });
  // a scanner in USB-COM mode reads into the same lookup (HW-020); a dialog that reads its own
  // scans gets them in its scan field instead
  useSerialScans(onScan, till.data !== null && !ownScanField);
  useCustomerDisplayFeed(dialog?.kind === "scale" ? weighing : null);

  // Voids and cancellations send the cart as it stands and the backend values it (AUD-001): a
  // preview may still hold an older cart, or none when it is refused. One at a time, so that a
  // held-down Delete cannot journal a line twice or remove a line nobody journaled.
  const journaling = useRef(false);
  const runApproved = approval.run;

  const voidLine = useCallback(
    async (index: number): Promise<boolean> => {
      const state = useCart.getState();
      const draft = state.lines[index];
      if (!draft || journaling.current) return false;
      const event = { kind: "line_voided", cart: toCart(state), index } as const;
      journaling.current = true;
      try {
        await runApproved(() => recordEvent.mutateAsync(event));
        const now = useCart.getState();
        const at = now.lines.findIndex((l) => l.key === draft.key);
        if (at >= 0) now.remove(at);
        return true;
      } catch (error) {
        if (!isApprovalCancelled(error)) notify("error", errorText(error));
        return false;
      } finally {
        journaling.current = false;
      }
    },
    [errorText, notify, recordEvent, runApproved],
  );

  // Lowering a line's quantity is a partial void (SAL-040): journaled with the backend's figures
  // and under `sales.void_line` before the line changes. `false` when refused or cancelled.
  const reduceQty = useCallback(
    async (index: number, qty: number): Promise<boolean> => {
      const state = useCart.getState();
      const draft = state.lines[index];
      if (!draft || journaling.current || qty <= 0 || qty >= draft.line.qty) return false;
      const event = { kind: "qty_reduced", cart: toCart(state), index, qty } as const;
      journaling.current = true;
      try {
        await runApproved(() => recordEvent.mutateAsync(event));
        const now = useCart.getState();
        const at = now.lines.findIndex((l) => l.key === draft.key);
        if (at >= 0) now.setQty(at, qty);
        return true;
      } catch (error) {
        if (!isApprovalCancelled(error)) notify("error", errorText(error));
        return false;
      } finally {
        journaling.current = false;
      }
    },
    [errorText, notify, recordEvent, runApproved],
  );

  const cancelSale = useCallback(async () => {
    const state = useCart.getState();
    if (state.lines.length === 0 || journaling.current) return;
    const event = { kind: "cart_cancelled", cart: toCart(state) } as const;
    journaling.current = true;
    try {
      await runApproved(() => recordEvent.mutateAsync(event));
      useCart.getState().clear(docLang);
      notify("info", t("pos:cancelled"));
    } catch (error) {
      if (!isApprovalCancelled(error)) notify("error", errorText(error));
    } finally {
      journaling.current = false;
    }
  }, [docLang, errorText, notify, recordEvent, runApproved, t]);

  // Spec §10.4 "Undo hold" and F7: a held ticket comes back into an empty cart only, since
  // loading it would replace the lines on screen without a journal row. This screen owns the
  // mutation because the hold dialog is gone by the time its toast is clicked.
  const recallCart = recall.mutateAsync;
  const recallHeld = useCallback(
    async (id: string) => {
      if (useCart.getState().lines.length > 0) {
        notify("error", t("pos:held.busy"));
        return;
      }
      if (useCart.getState().exchange) {
        notify("error", t("pos:returns.exchangeFirst"));
        return;
      }
      const recalled = await recallCart(id);
      loadRecalled(recalled, notify, t);
      setDialog((current) => (current?.kind === "held" ? null : current));
    },
    [notify, recallCart, t],
  );

  // A pending exchange belongs to this customer's ticket: holding or recalling a ticket now would
  // drop the return, or pay another customer's ticket with it.
  const openDialog = useCallback(
    (next: PosDialog) => {
      if ((next.kind === "hold" || next.kind === "held") && useCart.getState().exchange) {
        notify("error", t("pos:returns.exchangeFirst"));
        return;
      }
      setDialog(next);
    },
    [notify, t],
  );

  const hold = () => {
    if (useCart.getState().lines.length === 0) notify("info", t("pos:hold.empty"));
    else openDialog({ kind: "hold" });
  };

  const commitDone = useCommitDone((done) => setDialog({ kind: "done", done }));

  // Settings › Payments, "one method": the sale is finished at once in that method, for the exact
  // amount the backend asks (in cash, the stamp duty and the cash rounding included), through the
  // same path as the payment dialog's Valider. A pending exchange (its store credit), an unknown
  // printer default or a method that cannot settle the whole amount opens the dialog instead; a
  // refusal opens it on the tender tried, with the backend's answer.
  const takePayment = async () => {
    const fixed = tillPayment.fixed;
    if (quickPaying.current) return;
    if (!fixed || useCart.getState().exchange || !hardware.isSuccess) {
      setDialog({ kind: "pay" });
      return;
    }
    quickPaying.current = true;
    setQuickBusy(true);
    let tenders: Tender[] | undefined;
    try {
      // UX-004: the receipt in the language of the customer's file, as the dialog prints it
      const customerId = useCart.getState().customerId;
      const customerLang = customerId ? (await fetchCustomer(customerId)).lang : null;
      if (customerLang) useCart.getState().setLang(customerLang);
      const cart = toCart(useCart.getState());
      const due = await previewSale({ cart, tenders: [], storeCredit: null });
      const tender = newTender(
        fixed.method,
        formatAmountInput(exactAmount(fixed.method, due.settlement), lang),
      );
      tenders = [
        fixed.type
          ? { ...tender, type: { id: fixed.type.id, name: localizedName(fixed.type, lang) } }
          : tender,
      ];
      const inputs = toInputs(tenders) ?? [];
      const check = await previewSale({ cart, tenders: inputs, storeCredit: null });
      if (check.settlement.missing !== 0) {
        setDialog({ kind: "pay", tenders });
        return;
      }
      const done = await finishSale.finish({
        tenders,
        inputs,
        printReceipt: receiptByDefault(hardware.data),
        exchange: null,
      });
      if (done) commitDone(done);
    } catch (error) {
      if (!isApprovalCancelled(error)) setDialog({ kind: "pay", tenders, error });
    } finally {
      quickPaying.current = false;
      setQuickBusy(false);
    }
  };

  // "Encaisser": with a pack module that asks something of the sale (serials, a prescription, the
  // buyer of a jewel), its checkout steps come first (`till-packs.tsx`), then the payment
  const pay = () => {
    if (useCart.getState().lines.length === 0 || preview.error || quickPaying.current) return;
    const steps = modules.some((m) => CHECKOUT_MODULES.includes(m));
    if (steps) setDialog({ kind: "pack", view: { kind: "checkout" } });
    else void takePayment();
  };

  const toggleDepartments = () => {
    rememberDepartments(!departments);
    setDepartmentsOn(!departments);
  };
  const flexy = () => setDialog({ kind: "pack", view: { kind: "flexy" } });

  // UX-017: every action through the till's key map; a pack action only while its module is on
  // (Ctrl+3, the gift voucher, is a pack dialog bound here)
  const handlers: Partial<Record<TillAction, () => void>> = {
    help: () => setDialog({ kind: "help" }),
    search: () => scanRef.current?.focus(),
    customer: () => setDialog({ kind: "customer" }),
    price_check: () => setDialog({ kind: "price" }),
    discount: () => setDialog({ kind: "discount" }),
    hold,
    held: () => openDialog({ kind: "held" }),
    return: () => setDialog({ kind: "return" }),
    drawer: () => setDialog({ kind: "drawer" }),
    repeat_last: () => useCart.getState().repeatLast(),
    checkout: pay,
    menu: () => setDialog({ kind: "menu" }),
  };
  if (modules.includes("restaurant")) handlers.floor = () => void navigate("/pos/floor");
  if (modules.includes("flexy")) handlers.flexy = flexy;
  if (modules.includes("department_keys")) handlers.department_keys = toggleDepartments;
  // LOY-001: a gift voucher sold from the till (a pack dialog, its key from the map)
  if (modules.includes("gift_vouchers") && can("sales.create")) {
    handlers.gift_voucher = () => setDialog({ kind: "giftVoucher" });
  }
  if (modules.includes("serials")) {
    handlers.serial = () => {
      const line = useCart.getState().selected;
      if (line !== null) setDialog({ kind: "pack", view: { kind: "serial", line } });
    };
  }
  const bound = (Object.keys(DEFAULT_KEYS) as TillAction[]).filter((action) => handlers[action]);
  const hotkeys: Partial<Record<Hotkey, () => void>> = { F11: () => setDialog({ kind: "scale" }) };
  for (const action of bound) hotkeys[keys[action]] = handlers[action];
  useHotkeys(hotkeys, dialog === null && !approval.pending && !quickBusy);
  /** A button's keycap from the key map. */
  const cap = (action: TillAction) => ({
    shortcut: ariaKey(keys[action]),
    shortcutLabel: keyLabel(keys[action], t),
  });
  // SAL-046: a quote or proforma into the empty cart
  useCtrlHotkey(
    "d",
    () => openDialog({ kind: "quote" }),
    dialog === null && !approval.pending && can("sales.create"),
  );

  const ctx: PosContext = {
    session,
    preview: preview.data,
    run: approval.run,
    voidLine,
    reduceQty,
    cancelSale,
    recallHeld,
    notify,
    open: openDialog,
    categoryId: category,
    addWeighed,
    onWeighing: setWeighing,
    modules,
    keys,
    actions: bound,
    addProduct,
    checkoutDone: () => {
      setDialog(null);
      void takePayment();
    },
    toggleDepartments,
  };

  // Pharmacy: a cheaper generic replaces the line as a void (journaled, AUD-001) and a new line
  const replaceLine = async (index: number, generic: TillProduct) => {
    const draft = useCart.getState().lines[index];
    if (!draft) return;
    if (await voidLine(index)) addProduct(generic, draft.line.qty, unitDecimalsOf(generic, units));
  };

  // Focus goes back to the scan field after every tap on the screen (a tile, a line, a chip), so
  // the keyboard model (+, -, *, Del, typed codes) keeps working (UX-010).
  const refocus = (event: PointerEvent<HTMLDivElement>) => {
    if (dialog !== null || approval.pending) return;
    const target = event.target as HTMLElement;
    if (target.closest("input, textarea, select, [contenteditable='true']")) return;
    window.setTimeout(() => {
      if (!document.querySelector("[role='dialog']")) scanRef.current?.focus();
    }, 0);
  };

  const closed = till.isSuccess && till.data === null;
  const closedView = (
    <div className="grid flex-1 place-items-center p-6">
      <EmptyState
        icon={Lock}
        title={t("pos:closed.title")}
        body={t("pos:closed.body")}
        action={
          <div className="flex gap-2">
            {can("cash.open") ? (
              <Button variant="primary" size="lg" onClick={() => setDialog({ kind: "openSession" })}>
                <Wallet aria-hidden />
                {t("pos:closed.open")}
              </Button>
            ) : null}
            <Button variant="outline" size="lg" onClick={() => void navigate(backOfficeHome(session))}>
              {t("pos:closed.back")}
            </Button>
          </div>
        }
      />
    </div>
  );

  const pos = "min-h-14 flex-col gap-0.5 px-2 text-[1.125rem] [&_kbd]:ms-0";
  const actions = (
    <>
      <Button className={pos} onClick={() => setDialog({ kind: "customer" })} {...cap("customer")}>
        <User aria-hidden />
        {t("pos:actions.customer")}
      </Button>
      <Button className={pos} onClick={() => setDialog({ kind: "discount" })} {...cap("discount")}>
        <Percent aria-hidden />
        {t("pos:actions.discount")}
      </Button>
      <Button className={pos} onClick={hold} disabled={!can("sales.hold")} {...cap("hold")}>
        <Pause aria-hidden />
        {t("pos:actions.hold")}
      </Button>
      <Button className={pos} onClick={() => setDialog({ kind: "return" })} {...cap("return")}>
        <Undo2 aria-hidden className="rtl:-scale-x-100" />
        {t("pos:actions.return")}
      </Button>
      <Button
        className={pos}
        onClick={() => setDialog({ kind: "drawer" })}
        disabled={!can("cash.paid_in_out")}
        {...cap("drawer")}
      >
        <Wallet aria-hidden />
        {t("pos:actions.drawer")}
      </Button>
      <Button className={pos} onClick={() => setDialog({ kind: "menu" })} {...cap("menu")}>
        <Menu aria-hidden />
        {t("pos:actions.menu")}
      </Button>
    </>
  );

  const heldCount = held.data?.length ?? 0;
  // One tree for the open and the closed till, so the backup alerts (the "database damaged"
  // alert above all) keep their place and state when the till's state arrives.
  return (
    <div className="flex h-dvh flex-col bg-bg text-[1.125rem] text-ink" onPointerUp={refocus}>
      <DemoRibbon />
      <BackupAlerts session={session} />
      <LanBanner />
      {van ? (
        <Suspense fallback={null}>
          <VanBanner />
        </Suspense>
      ) : null}
      {closed ? (
        closedView
      ) : (
        <div className="flex min-h-0 flex-1">
          <main className="flex min-w-0 flex-1 flex-col gap-3 p-4">
            <ScanField
              inputRef={scanRef}
              value={text}
              onValueChange={setText}
              multiplier={multiplier}
              onCode={(code) => void lookup(code)}
              // with one payment method set, Enter on the empty field pays like F12
              onEmptyEnter={tillPayment.fixed ? pay : undefined}
              onPick={pickProduct}
              onMultiplier={(qty) => useCart.getState().setMultiplier(qty)}
              onBump={(direction) => {
                const state = useCart.getState();
                const index = state.selected;
                const line = index === null ? undefined : state.lines[index];
                if (index === null || !line) return;
                if (direction === 1) state.bump(index, 1);
                else if (line.line.qty > 1_000) void reduceQty(index, line.line.qty - 1_000);
              }}
              onVoid={() => {
                const index = useCart.getState().selected;
                if (index !== null) void voidLine(index);
              }}
              onMove={(delta) => useCart.getState().move(delta)}
            />
            {notice ? (
              <p
                role={notice.tone === "error" ? "alert" : "status"}
                className={cn(
                  "rounded-control p-3 font-semibold",
                  notice.tone === "error" ? "bg-danger-soft text-danger" : "bg-info-soft text-info",
                )}
              >
                {notice.text}
              </p>
            ) : lastScan ? (
              <p className="flex items-center justify-between rounded-control bg-surface p-3 text-lg">
                <span>{t("pos:lastScan", { name: lastScan.name })}</span>
                <Amount value={lastScan.price} className="font-bold" />
              </p>
            ) : null}
            {modules.includes("department_keys") || modules.includes("flexy") ? (
              <div className="flex flex-wrap gap-2">
                {modules.includes("department_keys") ? (
                  <Button
                    variant={departments ? "secondary" : "outline"}
                    aria-pressed={departments}
                    onClick={toggleDepartments}
                    {...cap("department_keys")}
                  >
                    {t("pos:actions.departments")}
                  </Button>
                ) : null}
                {modules.includes("flexy") ? (
                  <Button variant="outline" onClick={flexy} {...cap("flexy")}>
                    {t("pos:actions.flexy")}
                  </Button>
                ) : null}
              </div>
            ) : null}
            {departments ? (
              <Suspense fallback={null}>
                <DepartmentKeysPanel />
              </Suspense>
            ) : (
              <ProductGrid onPick={pickProduct} category={category} onCategory={setCategory} />
            )}
          </main>
          <CartPanel
            lines={lines}
            selected={selected}
            preview={preview.data}
            priced={priced}
            stale={preview.isPlaceholderData}
            error={preview.error}
            customer={<CustomerStrip session={session} onOpen={() => setDialog({ kind: "customer" })} />}
            onSelect={(index) => useCart.getState().select(index)}
            onEdit={(index) => setDialog({ kind: "edit", index })}
            notices={
              modules.includes("pharmacy")
                ? (index, productId) => (
                    <Suspense fallback={null}>
                      <LineNotices
                        productId={productId}
                        onReplace={(generic) => void replaceLine(index, generic)}
                      />
                    </Suspense>
                  )
                : undefined
            }
            onApproval={() =>
              void approval
                .run(async () => {
                  const result = await preview.refetch();
                  if (result.error) throw result.error;
                  return result.data;
                })
                .catch(() => undefined)
            }
            payButton={
              <Button
                variant="primary"
                className="min-h-16 w-full text-2xl"
                disabled={lines.length === 0 || Boolean(preview.error) || quickBusy}
                onClick={pay}
                {...cap("checkout")}
              >
                {t("payments:pay")}
              </Button>
            }
            banner={
              <div className="flex items-center gap-2">
                {withTables ? (
                  <Button variant="outline" onClick={() => void navigate("/pos/floor")}>
                    {t("pos:actions.tables")}
                  </Button>
                ) : null}
                {hardware.isSuccess ? (
                  <PrinterDot
                    configured={Boolean(hardware.data.receiptPrinterId)}
                    onOpen={() => openDialog({ kind: "unprinted" })}
                  />
                ) : null}
                <SyncPill session={session} />
                {fromQuote ? (
                  <span role="status" className="rounded-full bg-info-soft px-3 py-1 font-semibold text-info">
                    {t("billing:offer.chip", { number: fromQuote.number })}
                  </span>
                ) : null}
                {exchange ? (
                  <span
                    role="status"
                    aria-label={t("pos:returns.exchangeActive")}
                    className="flex items-center gap-1 rounded-full bg-info-soft ps-3 font-semibold text-info"
                  >
                    {t("pos:returns.exchangeChip")} <Amount value={exchange.value} />
                    <Button
                      variant="ghost"
                      size="icon"
                      className="rounded-full"
                      aria-label={t("pos:returns.cancelExchange")}
                      title={t("pos:returns.cancelExchange")}
                      onClick={() => useCart.getState().startExchange(null)}
                    >
                      <X aria-hidden />
                    </Button>
                  </span>
                ) : null}
                {hardware.data?.scale ? (
                  <Button
                    variant="outline"
                    onClick={() => setDialog({ kind: "scale" })}
                    shortcut="F11"
                    shortcutLabel={t("common:keys.f11")}
                  >
                    <Scale aria-hidden />
                    {t("pos:weigh.title")}
                  </Button>
                ) : null}
                {heldCount > 0 ? (
                  <Button variant="outline" onClick={() => openDialog({ kind: "held" })} {...cap("held")}>
                    {t("pos:actions.held", { count: heldCount })}
                  </Button>
                ) : null}
              </div>
            }
            actions={actions}
          />
        </div>
      )}
      <PosDialogs dialog={dialog} onClose={() => setDialog(null)} ctx={ctx} />
      {approval.dialog}
    </div>
  );
}
