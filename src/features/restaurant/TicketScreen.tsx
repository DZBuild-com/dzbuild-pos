import { ArrowLeft } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useParams } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useTillKit } from "@/features/pos/till-kit";
import { useScannerOptions } from "@/lib/ipc/hardware";
import { useFindByCode, useProductGrid } from "@/lib/ipc/products";
import {
  useAddLine,
  useDailyMenu,
  useFetchModifiers,
  usePayTicket,
  useRemoveUnsent,
  useSendToKitchen,
  useSetQty,
  useTicket,
} from "@/lib/ipc/restaurant";
import type { ModifierGroup, PaymentInput, Session, Ticket, TicketLine } from "@/lib/ipc/types";
import { localizedName, type Trilingual } from "@/lib/names";
import { useScanner } from "@/lib/scanner";
import { beep } from "@/lib/sounds";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { counterName } from "./CounterTickets";
import { ModifierDialog } from "./ModifierDialog";
import { MoveDialog } from "./MoveDialog";
import { PrebillDialog } from "./PrebillDialog";
import { SplitDialog } from "./SplitDialog";
import { LineEditDialog, PlacePickers, TicketPanel } from "./TicketPanel";
import { CompDialog, VoidDialog } from "./VoidDialog";

/** A product the waiter tapped or scanned. */
interface Picked extends Trilingual {
  id: string;
  kind: string;
}

type TicketDialog =
  | { kind: "modifiers"; product: Picked; qty: number; groups: ModifierGroup[] }
  | { kind: "edit"; line: TicketLine }
  | { kind: "void"; line: TicketLine }
  | { kind: "comp"; line: TicketLine }
  | { kind: "prebill" }
  | { kind: "split" }
  | { kind: "move"; mode: "transfer" | "merge" }
  | { kind: "pay" }
  /** Leaving the ticket with lines not sent to the kitchen (final review). */
  | { kind: "leave" };

interface Notice {
  tone: "info" | "error";
  text: string;
}

/** `/pos/ticket/:id`. */
export function TicketRoute({ session }: { session: Session }) {
  const { id } = useParams();
  return id ? <TicketScreen key={id} session={session} ticketId={id} /> : null;
}

/** The ticket's title: its table, or its number and kind at the counter. */
function useTitle(ticket: Ticket | undefined): string {
  const { t } = useTranslation();
  if (!ticket) return t("restaurant:floor.title");
  return ticket.tableCode === null
    ? counterName(ticket, t)
    : t("restaurant:open.title", { code: ticket.tableCode });
}

/** `/pos/ticket/:id` (RST-002…009, SAL-024): M1b's scan field and product grid (the daily menu
 * first) at the start side, the table ticket at the end side. A product with options asks them
 * first; lines go to the chosen seat and course. Envoyer en cuisine (F6, the sale screen's
 * "Attente": the ticket waits for its next course), Addition, Diviser, Transférer, Fusionner; a
 * sent line is voided (F8) or offered (F5) with a reason; Encaisser (F12) is M1b's payment dialog
 * paying the ticket's bill, then the floor (Esc). F2 brings the keyboard back to the scan field.
 * The ticket is always the server's. */
export function TicketScreen({ session, ticketId }: { session: Session; ticketId: string }) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const { DemoRibbon, LanBanner, PaymentDialog, ProductGrid, ScanField, useHotkeys } = useTillKit();
  const query = useTicket(ticketId);
  const ticket = query.data;
  const title = useTitle(ticket);
  const menu = useDailyMenu(ticket?.businessDate ?? "", ticket !== undefined);
  const grid = useProductGrid(null);
  const fetchModifiers = useFetchModifiers();
  const addLine = useAddLine();
  const setQty = useSetQty();
  const remove = useRemoveUnsent();
  const send = useSendToKitchen();
  const payTicket = usePayTicket();
  const findByCode = useFindByCode();
  const scanner = useScannerOptions();
  const [seat, setSeat] = useState(0);
  const [course, setCourse] = useState<number | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [dialog, setDialog] = useState<TicketDialog | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [text, setText] = useState("");
  const [multiplier, setMultiplier] = useState<number | null>(null);
  const [category, setCategory] = useState<string | null>(null);
  const scanRef = useRef<HTMLInputElement>(null);
  const may = (permission: Session["permissions"][number]) => session.permissions.includes(permission);
  const floor = useCallback(() => void navigate("/pos/floor"), [navigate]);
  const fail = useCallback(
    (error: unknown) => {
      beep("error");
      setNotice({ tone: "error", text: errorText(error) });
    },
    [errorText],
  );

  const live = ticket !== undefined && (ticket.status === "open" || ticket.status === "billed");
  const mains = ticket?.lines.filter((l) => l.parentLineId === null) ?? [];
  const unsent = mains.some((l) => l.sentQty < l.qty);
  const selectedLine = mains.find((l) => l.id === selected);

  const add = async (product: Picked, qty: number, modifiers: string[]) => {
    try {
      await addLine.mutateAsync({
        ticketId,
        input: {
          cartLine: {
            productId: product.id,
            description: null,
            qty,
            unitPrice: null,
            discount: { kind: "none" },
            taxRateId: null,
            note: null,
            expectedUnitPrice: null,
          },
          seat: seat === 0 ? null : seat,
          course,
          modifiers,
        },
      });
      beep("ok");
      setMultiplier(null);
      setNotice(null);
      setDialog(null);
    } catch (error) {
      fail(error);
    }
  };

  // SAL-024: a product with modifier groups asks them before its line is added
  const pick = async (product: Picked, qty = multiplier ?? 1_000) => {
    if (!live) return;
    if (product.kind === "variant_parent") {
      beep("error");
      setNotice({ tone: "error", text: t("pos:scan.variantParent") });
      return;
    }
    try {
      const groups = (await fetchModifiers(product.id)).filter((g) => !g.archived);
      if (groups.length === 0) await add(product, qty, []);
      else setDialog({ kind: "modifiers", product, qty, groups });
    } catch (error) {
      fail(error);
    }
  };

  const lookup = async (code: string) => {
    setText("");
    try {
      const hit = await findByCode.mutateAsync(code);
      if (hit) await pick(hit.product, hit.packQty);
      else {
        beep("error");
        setNotice({ tone: "error", text: t("pos:scan.notFound", { code }) });
      }
    } catch (error) {
      fail(error);
    }
  };
  useScanner((code) => void lookup(code), { ...scanner, enabled: dialog === null && live });
  // UX-010, as on the sale screen: the keyboard comes back to the scan field whenever no dialog
  // holds it (an option chosen, a line voided), and goes to it once the ticket is read (the field
  // waits for it disabled)
  useEffect(() => {
    if (dialog === null && live) scanRef.current?.focus();
  }, [dialog, live]);

  const sendToKitchen = () => {
    if (!live || !unsent || send.isPending) return;
    send.mutate(ticketId, {
      onSuccess: () => setNotice({ tone: "info", text: t("restaurant:ticket.sentToKitchen") }),
      onError: fail,
    });
  };
  // final review: a table never leaves with dishes the kitchen has not been told about unawares
  const leave = () => {
    if (live && unsent) setDialog({ kind: "leave" });
    else floor();
  };
  const sendAndLeave = () => {
    if (send.isPending) return;
    send.mutate(ticketId, {
      onSuccess: () => {
        toast.success(t("restaurant:ticket.sentToKitchen"));
        floor();
      },
      onError: (error) => {
        setDialog(null);
        fail(error);
      },
    });
  };
  const pay = () => {
    if (live && mains.length > 0) setDialog({ kind: "pay" });
  };
  const payWith = async (payments: PaymentInput[], printReceipt: boolean) => {
    const done = await payTicket.mutateAsync({ input: { ticketId, payments }, printReceipt });
    toast.success(t("restaurant:ticket.paid", { number: done.paid.sale.document.number ?? "" }));
    floor();
  };
  const voidOrRemove = (line: TicketLine) => {
    if (line.sentQty > 0) setDialog({ kind: "void", line });
    else remove.mutate(line.id, { onError: fail });
  };

  useHotkeys(
    {
      F2: () => scanRef.current?.focus(),
      F6: sendToKitchen,
      F5: () =>
        selectedLine && !selectedLine.compReason
          ? setDialog({ kind: "comp", line: selectedLine })
          : undefined,
      F8: () => (selectedLine ? voidOrRemove(selectedLine) : undefined),
      F12: pay,
      Escape: leave,
    },
    dialog === null,
  );

  const menuIds = menu.data ?? [];
  const todays = menuIds.flatMap((id) => (grid.data ?? []).filter((hit) => hit.id === id));
  const dineIn = ticket?.mode === "dine_in";
  const secondary = "min-h-14 flex-col gap-0.5 px-2";

  return (
    <div className="flex h-dvh flex-col bg-bg text-[1.125rem] text-ink">
      <DemoRibbon />
      <LanBanner />
      <header className="flex flex-wrap items-center gap-3 border-b border-line bg-surface px-4 py-2">
        <Button variant="ghost" onClick={leave} shortcut="Escape" shortcutLabel={t("common:keys.escape")}>
          <ArrowLeft aria-hidden className="rtl:-scale-x-100" />
          {t("restaurant:floor.title")}
        </Button>
        <h1 className="text-2xl font-bold">{title}</h1>
        {ticket ? (
          <span className="text-ink-2">
            {t("restaurant:floor.covers", { count: ticket.covers })} · {ticket.waiterName}
          </span>
        ) : null}
      </header>
      {query.isError ? (
        <p role="alert" className="m-4 rounded-control bg-danger-soft p-3 font-semibold text-danger">
          {errorText(query.error)}
        </p>
      ) : null}
      {ticket && !live ? (
        <p role="status" className="m-4 rounded-control bg-info-soft p-3 font-semibold text-info">
          {t("restaurant:ticket.closed")}
        </p>
      ) : null}
      <div className="flex min-h-0 flex-1">
        <main className="flex min-w-0 flex-1 flex-col gap-3 p-4">
          <ScanField
            inputRef={scanRef}
            value={text}
            onValueChange={setText}
            multiplier={multiplier}
            onCode={(code) => void lookup(code)}
            onPick={(hit) => {
              setText("");
              void pick(hit);
            }}
            onMultiplier={setMultiplier}
            onBump={() => undefined}
            onVoid={() => (selectedLine ? voidOrRemove(selectedLine) : undefined)}
            onMove={() => undefined}
            disabled={!live}
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
          ) : null}
          {todays.length > 0 ? (
            <section aria-labelledby="daily-menu" className="flex flex-col gap-2">
              <h2 id="daily-menu" className="font-bold">
                {t("restaurant:admin.menu")}
              </h2>
              <div className="flex gap-2 overflow-x-auto pb-1">
                {todays.map((hit) => (
                  <button
                    key={hit.id}
                    type="button"
                    onClick={() => void pick(hit)}
                    className="flex min-h-14 shrink-0 flex-col items-start justify-center rounded-card border-2 border-brand bg-brand-soft px-3 text-start"
                  >
                    <span className="font-semibold">{localizedName(hit, lang)}</span>
                    <Amount value={hit.sellPrice} className="text-sm font-bold" />
                  </button>
                ))}
              </div>
            </section>
          ) : null}
          <ProductGrid onPick={(hit) => void pick(hit)} category={category} onCategory={setCategory} />
        </main>
        {ticket ? (
          <TicketPanel
            ticket={ticket}
            selected={selected}
            pickers={
              <PlacePickers
                ticket={ticket}
                seat={seat}
                course={course}
                onSeat={setSeat}
                onCourse={setCourse}
              />
            }
            actions={{
              onSelect: (line) => setSelected((now) => (now === line.id ? null : line.id)),
              onQty: (line, delta) =>
                setQty.mutate({ lineId: line.id, qty: line.qty + delta * 1_000 }, { onError: fail }),
              onRemove: (line) => remove.mutate(line.id, { onError: fail }),
              onEdit: (line) => setDialog({ kind: "edit", line }),
              onVoid: (line) => setDialog({ kind: "void", line }),
              onComp: (line) => setDialog({ kind: "comp", line }),
            }}
            footer={
              <div className="flex flex-col gap-2">
                <div className="grid grid-cols-2 gap-2">
                  <Button
                    variant={unsent ? "primary" : "secondary"}
                    className="min-h-16 text-lg"
                    disabled={!live || !unsent || send.isPending}
                    onClick={sendToKitchen}
                    shortcut="F6"
                    shortcutLabel={t("common:keys.f6")}
                  >
                    {t("restaurant:ticket.send")}
                  </Button>
                  <Button
                    variant={unsent ? "secondary" : "primary"}
                    className="min-h-16 text-lg"
                    disabled={!live || mains.length === 0}
                    onClick={pay}
                    shortcut="F12"
                    shortcutLabel={t("common:keys.f12")}
                  >
                    {t("restaurant:ticket.pay")}
                  </Button>
                </div>
                <div className="grid grid-cols-4 gap-2">
                  <Button
                    className={secondary}
                    disabled={!live || mains.length === 0}
                    onClick={() => setDialog({ kind: "prebill" })}
                  >
                    {t("restaurant:ticket.prebill")}
                  </Button>
                  {dineIn ? (
                    <Button
                      className={secondary}
                      disabled={!live || mains.length < 2}
                      onClick={() => setDialog({ kind: "split" })}
                    >
                      {t("restaurant:ticket.split")}
                    </Button>
                  ) : null}
                  {dineIn ? (
                    <Button
                      className={secondary}
                      disabled={!live}
                      onClick={() => setDialog({ kind: "move", mode: "transfer" })}
                    >
                      {t("restaurant:ticket.transfer")}
                    </Button>
                  ) : null}
                  <Button
                    className={secondary}
                    disabled={!live || mains.length === 0}
                    onClick={() => setDialog({ kind: "move", mode: "merge" })}
                  >
                    {t("restaurant:ticket.merge")}
                  </Button>
                </div>
              </div>
            }
          />
        ) : null}
      </div>
      {dialog?.kind === "modifiers" ? (
        <ModifierDialog
          title={localizedName(dialog.product, lang)}
          groups={dialog.groups}
          busy={addLine.isPending}
          onAdd={(options) => void add(dialog.product, dialog.qty, options)}
          onClose={() => setDialog(null)}
        />
      ) : null}
      {dialog?.kind === "edit" && ticket ? (
        <LineEditDialog ticket={ticket} line={dialog.line} onClose={() => setDialog(null)} />
      ) : null}
      {dialog?.kind === "void" ? (
        <VoidDialog line={dialog.line} may={may} onClose={() => setDialog(null)} />
      ) : null}
      {dialog?.kind === "comp" ? (
        <CompDialog line={dialog.line} may={may} onClose={() => setDialog(null)} />
      ) : null}
      {dialog?.kind === "prebill" ? (
        <PrebillDialog ticketId={ticketId} onClose={() => setDialog(null)} />
      ) : null}
      {dialog?.kind === "split" && ticket ? (
        <SplitDialog
          ticket={ticket}
          onClose={() => setDialog(null)}
          onSplit={(to) => {
            setDialog(null);
            toast.success(t("restaurant:split.done"));
            void navigate(`/pos/ticket/${to.id}`);
          }}
        />
      ) : null}
      {dialog?.kind === "move" && ticket ? (
        <MoveDialog
          ticket={ticket}
          mode={dialog.mode}
          onClose={() => setDialog(null)}
          onMoved={(id) => {
            setDialog(null);
            if (id === ticketId) floor();
            else void navigate(`/pos/ticket/${id}`);
          }}
        />
      ) : null}
      {dialog?.kind === "leave" ? (
        <Dialog
          open
          onOpenChange={(open) => (open ? undefined : setDialog(null))}
          title={t("restaurant:ticket.leave.title")}
          description={t("restaurant:ticket.leave.body")}
          footer={
            <>
              <Button variant="outline" onClick={floor}>
                {t("restaurant:ticket.leave.without")}
              </Button>
              <Button variant="primary" autoFocus disabled={send.isPending} onClick={sendAndLeave}>
                {t("restaurant:ticket.leave.send")}
              </Button>
            </>
          }
        >
          {null}
        </Dialog>
      ) : null}
      {dialog?.kind === "pay" && ticket ? (
        <PaymentDialog
          ticket={{ id: ticket.id, lang: ticket.lang, customerId: ticket.customerId }}
          onPay={payWith}
          onClose={() => setDialog(null)}
        />
      ) : null}
    </div>
  );
}
