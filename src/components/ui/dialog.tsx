import { X } from "lucide-react";
import { Dialog as DialogPrimitive } from "radix-ui";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

const SIZES = {
  md: "w-[min(40rem,calc(100vw-2rem))]",
  lg: "w-[min(52rem,calc(100vw-2rem))]",
  xl: "w-[min(72rem,calc(100vw-2rem))]",
} as const;

/** A press on a toast (a failed receipt's "Réimprimer") or on a key of the on-screen keyboard is
 * not a press outside the dialog: the toaster and the keyboard sit above every modal and keep their
 * own buttons. */
export function keepToastsOutside(event: { target: EventTarget | null; preventDefault: () => void }) {
  if (event.target instanceof Element && event.target.closest("[data-sonner-toaster], [data-osk-panel]")) {
    event.preventDefault();
  }
}

/** The close button every dialog and sheet carries at the end of its title row (44 px): a touch
 * screen has no Esc key, and the overlay around a wide dialog can be a thin strip. */
export function CloseButton() {
  const { t } = useTranslation();
  return (
    <DialogPrimitive.Close
      aria-label={t("common:actions.close")}
      title={t("common:actions.close")}
      className="grid size-11 shrink-0 place-items-center rounded-control text-ink-2 hover:bg-surface-2 focus-visible:ring-3 focus-visible:ring-brand focus-visible:outline-none"
    >
      <X aria-hidden className="size-6" />
    </DialogPrimitive.Close>
  );
}

/** Title (with the close button at its end), body, footer with the primary action at the end side
 * (spec §10.4). Title and footer stay in view; only the body scrolls. */
export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  footer,
  size = "md",
  dismissible = true,
  onEscapeKeyDown,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  footer?: ReactNode;
  size?: keyof typeof SIZES;
  /** `false` for an alert only its own button closes (no close button then). */
  dismissible?: boolean;
  /** Esc closes the dialog unless this calls `preventDefault()` (a wizard going back a step).
   * Radix reads Esc on the document before any key handler inside the dialog sees it. */
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
  children: ReactNode;
}) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-40 bg-black/40" />
        <DialogPrimitive.Content
          onPointerDownOutside={keepToastsOutside}
          onFocusOutside={keepToastsOutside}
          onInteractOutside={keepToastsOutside}
          onEscapeKeyDown={onEscapeKeyDown}
          className={cn(
            "fixed inset-0 bottom-[var(--osk-inset,0px)] z-50 m-auto flex h-fit max-h-[calc(94dvh-var(--osk-inset,0px))] flex-col gap-4 overflow-hidden rounded-card border border-line bg-surface p-6 text-ink shadow-overlay",
            SIZES[size],
          )}
        >
          <div className="flex shrink-0 items-start justify-between gap-3">
            <DialogPrimitive.Title className="pt-1 text-2xl font-bold">{title}</DialogPrimitive.Title>
            {dismissible ? <CloseButton /> : null}
          </div>
          <DialogPrimitive.Description className={description ? "shrink-0 text-ink-2" : "sr-only"}>
            {description ?? title}
          </DialogPrimitive.Description>
          <div className="-mx-1 flex min-h-0 flex-col gap-4 overflow-y-auto px-1">{children}</div>
          {footer ? (
            <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">{footer}</div>
          ) : null}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
