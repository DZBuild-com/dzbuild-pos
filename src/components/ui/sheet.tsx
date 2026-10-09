import { Dialog as DialogPrimitive } from "radix-ui";
import type { ReactNode } from "react";
import { CloseButton, keepToastsOutside } from "./dialog";

/** A panel sliding in from the end side (held tickets, document details), with a close button at
 * the end of its title row. */
export function Sheet({
  open,
  onOpenChange,
  title,
  description,
  footer,
  onEscapeKeyDown,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  footer?: ReactNode;
  /** Esc closes the sheet unless this calls `preventDefault()` (a search field clearing first). */
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
  children: ReactNode;
}) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-40 bg-black/40" />
        <DialogPrimitive.Content
          onPointerDownOutside={keepToastsOutside}
          onInteractOutside={keepToastsOutside}
          onEscapeKeyDown={onEscapeKeyDown}
          className="fixed inset-y-0 end-0 z-50 flex w-[min(30rem,100vw)] flex-col gap-4 border-s border-line bg-surface p-6 text-ink shadow-overlay"
        >
          <div className="flex shrink-0 items-start justify-between gap-3">
            <DialogPrimitive.Title className="pt-1 text-2xl font-bold">{title}</DialogPrimitive.Title>
            <CloseButton />
          </div>
          <DialogPrimitive.Description className={description ? "text-ink-2" : "sr-only"}>
            {description ?? title}
          </DialogPrimitive.Description>
          <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
          {footer ? <div className="flex flex-wrap justify-end gap-2">{footer}</div> : null}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
