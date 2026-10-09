import { Eraser } from "lucide-react";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { digitFromKey } from "@/components/pin-pad";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import type { Lang } from "@/lib/i18n";
import { useOpenTicket } from "@/lib/ipc/restaurant";
import type { DiningTable, OpenInput, Ticket } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

const KEYS = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"] as const;

/** Where the new ticket goes: a table (its covers on a keypad, 1–99) or a delivery (the address
 * and the phone the kitchen and the driver read; the phone is the ticket's label). */
export type OpenPlace = { kind: "table"; table: DiningTable } | { kind: "delivery" };

const BASE: Omit<OpenInput, "mode" | "lang"> = {
  tableId: null,
  label: null,
  covers: 0,
  customerId: null,
  deliveryAddress: null,
  priceLevelId: null,
};

/** RST-001/RST-003: opens a table with its covers, or a delivery ticket. */
export function OpenTicketDialog({
  place,
  lang,
  onClose,
  onOpened,
}: {
  place: OpenPlace;
  lang: Lang;
  onClose: () => void;
  onOpened: (ticket: Ticket) => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const open = useOpenTicket();
  // the table's seats are proposed; the first key typed replaces them
  const [covers, setCovers] = useState(place.kind === "table" ? String(place.table.seats) : "");
  const [fresh, setFresh] = useState(true);
  const [address, setAddress] = useState("");
  const [phone, setPhone] = useState("");
  const count = Number(covers);
  const valid =
    place.kind === "table" ? Number.isInteger(count) && count >= 1 && count <= 99 : address.trim() !== "";
  const press = (key: string) => {
    setCovers((now) => `${fresh ? "" : now}${key}`.replace(/^0+/, "").slice(0, 2));
    setFresh(false);
  };
  const erase = () => {
    setCovers((now) => (fresh ? "" : now.slice(0, -1)));
    setFresh(false);
  };
  const submit = () => {
    if (!valid || open.isPending) return;
    const input: OpenInput =
      place.kind === "table"
        ? { ...BASE, mode: "dine_in", tableId: place.table.id, covers: count, lang }
        : {
            ...BASE,
            mode: "delivery",
            deliveryAddress: address.trim(),
            label: phone.trim() === "" ? null : phone.trim(),
            lang,
          };
    open.mutate(input, { onSuccess: onOpened });
  };
  return (
    <Dialog
      open
      onOpenChange={(next) => (next ? undefined : onClose())}
      title={
        place.kind === "table"
          ? t("restaurant:open.title", { code: place.table.code })
          : t("restaurant:floor.delivery")
      }
      footer={
        <>
          <Button variant="outline" size="lg" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            disabled={!valid || open.isPending}
            onClick={submit}
            shortcut="Enter"
            shortcutLabel={t("common:keys.enter")}
          >
            {place.kind === "table" ? t("restaurant:open.submit") : t("restaurant:open.start")}
          </Button>
        </>
      }
    >
      <form
        noValidate
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        {place.kind === "table" ? (
          <CoversPad covers={covers} onPress={press} onErase={erase} onSubmit={submit} />
        ) : (
          <>
            <Field id="delivery-address" label={t("restaurant:open.address")}>
              <Input
                id="delivery-address"
                value={address}
                maxLength={200}
                autoFocus
                onChange={(event) => setAddress(event.target.value)}
              />
            </Field>
            <Field id="delivery-phone" label={t("restaurant:open.phone")}>
              <Input
                id="delivery-phone"
                type="tel"
                dir="ltr"
                value={phone}
                maxLength={40}
                onChange={(event) => setPhone(event.target.value)}
              />
            </Field>
          </>
        )}
        {open.error ? (
          <p role="alert" className="text-danger">
            {errorText(open.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}

/** The covers on a keypad, and on the keyboard (spec §10.5, like the PIN pad): the pad takes the
 * focus when it opens, a digit types, Backspace erases, Enter opens the table. */
function CoversPad({
  covers,
  onPress,
  onErase,
  onSubmit,
}: {
  covers: string;
  onPress: (key: string) => void;
  onErase: () => void;
  onSubmit: () => void;
}) {
  const { t } = useTranslation();
  const padRef = useRef<HTMLDivElement>(null);
  // React only honours `autoFocus` on form controls: the pad focuses itself
  useEffect(() => {
    padRef.current?.focus();
  }, []);
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const digit = digitFromKey(event);
    if (digit !== null && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      onPress(digit);
    } else if (event.key === "Backspace") {
      event.preventDefault();
      onErase();
    } else if (event.key === "Enter") {
      event.preventDefault();
      onSubmit();
    }
  };
  return (
    <div
      ref={padRef}
      role="group"
      aria-label={t("restaurant:open.covers")}
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="flex flex-col items-center gap-3 rounded-card p-2 outline-none focus-visible:ring-3 focus-visible:ring-focus"
    >
      <p className="font-semibold">{t("restaurant:open.covers")}</p>
      <output
        aria-live="polite"
        aria-label={t("restaurant:open.covers")}
        dir="ltr"
        className="min-w-24 rounded-control bg-surface-2 px-6 py-2 text-center text-4xl font-bold tabular-nums"
      >
        {covers === "" ? "—" : covers}
      </output>
      <div className="grid grid-cols-3 gap-2" dir="ltr">
        {KEYS.slice(0, 9).map((key) => (
          <PadKey key={key} label={key} onPress={() => onPress(key)} />
        ))}
        <button
          type="button"
          tabIndex={-1}
          aria-label={t("auth:pinPad.erase")}
          onClick={onErase}
          className="grid size-16 place-items-center rounded-control bg-surface-2 hover:bg-line"
        >
          <Eraser aria-hidden />
        </button>
        <PadKey label="0" onPress={() => onPress("0")} />
      </div>
    </div>
  );
}

/** A key of the covers pad: the pad itself has the keyboard (one Tab stop). */
function PadKey({ label, onPress }: { label: string; onPress: () => void }) {
  return (
    <button
      type="button"
      tabIndex={-1}
      onClick={onPress}
      className="grid size-16 place-items-center rounded-control bg-surface-2 text-2xl font-bold hover:bg-line"
    >
      {label}
    </button>
  );
}
