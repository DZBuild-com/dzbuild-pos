import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export function NativeSelect({ className, ...props }: ComponentProps<"select">) {
  return (
    <select
      className={cn(
        "h-11 w-full rounded-control border border-line bg-surface px-3 text-base text-ink",
        className,
      )}
      {...props}
    />
  );
}
