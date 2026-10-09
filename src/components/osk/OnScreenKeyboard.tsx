import { CornerDownLeft, Delete, KeyboardOff } from "lucide-react";
import { type ReactNode, type RefObject, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { type Key, LAYOUTS, type LayoutName } from "./layouts";
import { eraseBefore, type Field, keyboardFor, pressEnter, typeInto, wantsKeyboard } from "./type-into";

const inPanel = (node: EventTarget | null): boolean =>
  node instanceof Element && node.closest("[data-osk-panel]") !== null;

/** This PC's "Clavier à l'écran" choice (`applyDisplay`). */
const asked = (): boolean => document.documentElement.dataset.osk === "1";

/** A press anywhere on the panel keeps the focus (and the caret) in the field, and a modal dialog
 * that holds the field open (`keepToastsOutside` lets the panel through). */
const keepFocus = (event: { preventDefault: () => void }) => event.preventDefault();

function KeyButton({
  onPress,
  label,
  pressed,
  className,
  children,
}: {
  onPress: () => void;
  label?: string;
  pressed?: boolean;
  className?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      tabIndex={-1}
      aria-label={label}
      title={label}
      aria-pressed={pressed}
      onClick={onPress}
      className={cn(
        "grid min-h-14 min-w-12 touch-manipulation place-items-center rounded-control border border-line bg-surface px-2 text-lg font-semibold text-ink active:bg-brand-soft aria-pressed:bg-brand aria-pressed:text-brand-ink",
        className,
      )}
    >
      {children}
    </button>
  );
}

/** The keypad of a number field: digits, the decimal comma for an amount or a quantity. */
const KEYPAD_ROWS = [
  ["7", "8", "9"],
  ["4", "5", "6"],
  ["1", "2", "3"],
] as const;

/** The panel's height, published on `<html>` as `--osk-inset` while it is open: dialogs, the app's
 * main area and the full-screen forms keep their content above it (1024x768 screens). */
export const OSK_INSET = "--osk-inset";

function usePublishedHeight(panel: RefObject<HTMLDivElement | null>, open: boolean) {
  useLayoutEffect(() => {
    const node = panel.current;
    const root = document.documentElement;
    if (!open || !node) {
      root.style.removeProperty(OSK_INSET);
      return;
    }
    const publish = () =>
      root.style.setProperty(OSK_INSET, `${Math.ceil(node.getBoundingClientRect().height)}px`);
    publish();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(publish);
    observer?.observe(node);
    return () => {
      observer?.disconnect();
      root.style.removeProperty(OSK_INSET);
    };
  }, [panel, open]);
}

/** For touch tills: when this PC asked for it (`<html data-osk="1">`), a field that gets the focus
 * brings up a keyboard docked at the bottom of the screen: AZERTY or Arabic for text, a keypad for
 * a number or a PIN (`inputmode`). `data-osk="off"` fields (the scan field, the payment dialog's
 * amounts, which have keys of their own) never do. The panel is a new node at the end of `<body>`
 * for each field, so a modal dialog that hid the page from assistive tech (`aria-hidden`) before
 * its own field got the focus does not hide the keyboard. */
export function OnScreenKeyboard() {
  const { t } = useTranslation();
  const lang = useLang();
  const [target, setTarget] = useState<{ field: Field; serial: number } | null>(null);
  const [chosen, setChosen] = useState<LayoutName | null>(null);
  const [shift, setShift] = useState(false);
  const layout = chosen ?? (lang === "ar" ? "arabic" : "azerty");
  const panel = useRef<HTMLDivElement>(null);
  usePublishedHeight(panel, target !== null);
  // The field stays in view above the panel once it is laid out.
  useEffect(() => {
    if (!target) return;
    const frame = requestAnimationFrame(() => target.field.scrollIntoView?.({ block: "nearest" }));
    return () => cancelAnimationFrame(frame);
  }, [target]);

  useEffect(() => {
    let serial = 0;
    const show = (node: EventTarget | null) => {
      const field = asked() && wantsKeyboard(node) ? node : null;
      serial += 1;
      const next = serial;
      setTarget((prev) => (field === null ? null : prev?.field === field ? prev : { field, serial: next }));
    };
    const onFocusIn = (event: FocusEvent) => {
      if (!inPanel(event.target)) show(event.target);
    };
    const onFocusOut = (event: FocusEvent) => {
      if (!inPanel(event.relatedTarget)) show(null);
    };
    // A field focused before this mounted, or when the choice is switched on or off.
    const sync = () => show(document.activeElement);
    sync();
    const observer = new MutationObserver(sync);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-osk"] });
    document.addEventListener("focusin", onFocusIn);
    document.addEventListener("focusout", onFocusOut);
    return () => {
      observer.disconnect();
      document.removeEventListener("focusin", onFocusIn);
      document.removeEventListener("focusout", onFocusOut);
    };
  }, []);

  if (!target) return null;
  const { field } = target;
  const kind = keyboardFor(field) ?? "text";

  const type = (k: Key) => {
    typeInto(field, shift ? (k.shift ?? k.key) : k.key);
    setShift(false);
  };
  const hide = () => {
    setTarget(null);
    field.blur();
  };

  return createPortal(
    <div
      key={target.serial}
      ref={panel}
      data-osk-panel
      data-osk-kind={kind}
      role="group"
      aria-label={t("common:osk.label")}
      onPointerDown={keepFocus}
      onMouseDown={keepFocus}
      className="pointer-events-auto fixed inset-x-0 bottom-0 z-[80] flex flex-col gap-1 border-t border-line bg-surface-2 p-2 select-none"
    >
      {kind === "text" ? null : (
        <Keypad
          decimal={kind === "decimal"}
          onType={(key) => typeInto(field, key)}
          onErase={() => eraseBefore(field)}
          onEnter={() => pressEnter(field)}
          onHide={hide}
        />
      )}
      {kind !== "text"
        ? null
        : LAYOUTS[layout].map((keys, i) => (
            <div
              key={`${layout}-${i}`}
              dir={layout === "arabic" && i > 0 ? "rtl" : "ltr"}
              className="flex justify-center gap-1"
            >
              {keys.map((k: Key) => {
                const char = shift ? (k.shift ?? k.key) : k.key;
                return (
                  <KeyButton key={k.key} onPress={() => type(k)} className="max-w-20 flex-1">
                    {char}
                  </KeyButton>
                );
              })}
            </div>
          ))}
      {kind !== "text" ? null : (
        <div className="flex justify-center gap-1">
          <KeyButton onPress={() => setShift((on) => !on)} pressed={shift} className="min-w-24">
            {t("common:osk.shift")}
          </KeyButton>
          <KeyButton onPress={() => typeInto(field, " ")} className="max-w-md flex-1">
            {t("common:osk.space")}
          </KeyButton>
          <KeyButton onPress={() => eraseBefore(field)} label={t("common:osk.erase")} className="min-w-20">
            <Delete aria-hidden className="size-6 rtl:-scale-x-100" />
          </KeyButton>
          <KeyButton onPress={() => pressEnter(field)} className="min-w-24">
            <span className="flex items-center gap-2">
              <CornerDownLeft aria-hidden className="size-5 rtl:-scale-x-100" />
              {t("common:osk.enter")}
            </span>
          </KeyButton>
          <KeyButton
            onPress={() => setChosen(layout === "arabic" ? "azerty" : "arabic")}
            className="min-w-24"
          >
            {layout === "arabic" ? t("common:osk.toAzerty") : t("common:osk.toArabic")}
          </KeyButton>
          <KeyButton onPress={hide} label={t("common:osk.hide")} className="min-w-16">
            <KeyboardOff aria-hidden className="size-6" />
          </KeyButton>
        </div>
      )}
    </div>,
    document.body,
  );
}

/** Digits (left to right whatever the language, UX-014), the decimal comma for an amount, then
 * Effacer, Entrée and Masquer. */
function Keypad({
  decimal,
  onType,
  onErase,
  onEnter,
  onHide,
}: {
  decimal: boolean;
  onType: (key: string) => void;
  onErase: () => void;
  onEnter: () => void;
  onHide: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div dir="ltr" className="mx-auto grid w-full max-w-md grid-cols-3 gap-1">
      {KEYPAD_ROWS.flat().map((key) => (
        <KeyButton key={key} onPress={() => onType(key)}>
          {key}
        </KeyButton>
      ))}
      {decimal ? <KeyButton onPress={() => onType(",")}>,</KeyButton> : <span aria-hidden />}
      <KeyButton onPress={() => onType("0")}>0</KeyButton>
      <KeyButton onPress={onErase} label={t("common:osk.erase")}>
        <Delete aria-hidden className="size-6" />
      </KeyButton>
      <KeyButton onPress={onEnter} className="col-span-2">
        <span className="flex items-center gap-2">
          <CornerDownLeft aria-hidden className="size-5" />
          {t("common:osk.enter")}
        </span>
      </KeyButton>
      <KeyButton onPress={onHide} label={t("common:osk.hide")}>
        <KeyboardOff aria-hidden className="size-6" />
      </KeyButton>
    </div>
  );
}
