import { cn, initials } from "@/lib/utils";

export function Avatar({ name, size = "md" }: { name: string; size?: "md" | "lg" }) {
  return (
    <span
      aria-hidden
      className={cn(
        "grid shrink-0 place-items-center rounded-full bg-brand-soft font-bold text-ink",
        size === "lg" ? "size-16 text-2xl" : "size-11 text-lg",
      )}
    >
      {initials(name)}
    </span>
  );
}
