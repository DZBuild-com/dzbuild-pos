import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export function Input({ className, ...props }: ComponentProps<"input">) {
  return (
    <input
      data-slot="input"
      className={cn(
        "h-11 w-full min-w-0 rounded-control border border-line bg-surface px-3 text-base text-ink disabled:opacity-50 aria-invalid:border-danger",
        className,
      )}
      {...props}
    />
  );
}
