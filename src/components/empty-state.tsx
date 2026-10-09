import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

/** Says what to do next in one sentence, with a button (spec §10.4). */
export function EmptyState({
  icon: Icon,
  title,
  body,
  action,
}: {
  icon: LucideIcon;
  title: string;
  body: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-3 rounded-card border border-dashed border-line p-10 text-center">
      <Icon aria-hidden className="size-10 text-ink-2" />
      <h2 className="text-xl font-bold">{title}</h2>
      <p className="max-w-prose text-ink-2">{body}</p>
      {action}
    </div>
  );
}
