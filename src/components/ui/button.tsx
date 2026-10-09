import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export const buttonVariants = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 rounded-control font-semibold whitespace-nowrap transition-colors select-none disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-5 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary: "bg-brand text-brand-ink hover:bg-brand/85",
        secondary: "bg-surface-2 text-ink hover:bg-line",
        outline: "border border-line bg-surface text-ink hover:bg-surface-2",
        ghost: "text-ink hover:bg-surface-2",
        danger: "border border-danger bg-danger-soft text-danger hover:bg-danger-soft/70",
      },
      size: { md: "min-h-11 px-4 text-base", lg: "min-h-14 px-5 text-lg", icon: "size-11" },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

export type ButtonProps = ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & { asChild?: boolean; shortcut?: string; shortcutLabel?: string };

/**
 * Spec §10.4: one `primary` per screen. `shortcut` is the key name for `aria-keyshortcuts`
 * ("Enter", "Escape", "F12"); the end-side keycap shows `shortcutLabel` when the key has a
 * translated name (`t("common:keys.enter")` → "Entrée" / "إدخال"), else `shortcut`. The keycap is
 * kept out of the accessible name ("Autoriser", not "AutoriserEntrée"): `aria-keyshortcuts` announces it.
 */
export function Button({
  className,
  variant,
  size,
  asChild = false,
  shortcut,
  shortcutLabel,
  children,
  type,
  ...props
}: ButtonProps) {
  if (asChild) {
    return (
      <Slot.Root data-slot="button" className={cn(buttonVariants({ variant, size }), className)} {...props}>
        {children}
      </Slot.Root>
    );
  }
  return (
    <button
      data-slot="button"
      type={type ?? "button"}
      aria-keyshortcuts={shortcut}
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    >
      {children}
      {shortcut ? (
        <kbd
          aria-hidden
          dir="ltr"
          className="ms-auto rounded-sm border border-current/30 px-1.5 text-sm font-normal opacity-80"
        >
          {shortcutLabel ?? shortcut}
        </kbd>
      ) : null}
    </button>
  );
}
