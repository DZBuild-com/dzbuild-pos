import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export function Card({ className, ...props }: ComponentProps<"section">) {
  return (
    <section
      className={cn("flex flex-col gap-4 rounded-card border border-line bg-surface p-6", className)}
      {...props}
    />
  );
}
