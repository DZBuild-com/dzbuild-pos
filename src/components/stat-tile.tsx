import { Minus, TrendingDown, TrendingUp } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** A change against a named period ("+12 % vs hier"): the arrow and the words carry it, the
 * colour only confirms whether it is good news. */
export interface Trend {
  direction: "up" | "down" | "flat";
  label: string;
  tone?: "good" | "bad" | "neutral";
}

const TREND_ICONS = { up: TrendingUp, down: TrendingDown, flat: Minus } as const;
const TREND_TONES = { good: "text-ok", bad: "text-danger", neutral: "text-ink-2" } as const;

/** One figure of a dashboard: its label, the value, an optional hint and trend. */
export function StatTile({
  label,
  value,
  hint,
  trend,
}: {
  label: string;
  value: ReactNode;
  hint?: string;
  trend?: Trend;
}) {
  const Icon = trend ? TREND_ICONS[trend.direction] : null;
  return (
    <dl className="flex min-w-0 flex-col gap-1 rounded-card border border-line bg-surface p-4">
      <dt className="text-sm text-ink-2">{label}</dt>
      {/* a six-figure amount fits a 1366 px screen: smaller type below xl, wrapped not clipped */}
      <dd className="min-w-0 text-xl leading-tight font-bold [overflow-wrap:anywhere] xl:text-2xl">
        {value}
      </dd>
      {trend && Icon ? (
        <dd
          className={cn(
            "flex items-center gap-1 text-sm font-semibold",
            TREND_TONES[trend.tone ?? "neutral"],
          )}
        >
          {/* time runs to the start side in Arabic */}
          <Icon aria-hidden className="size-4 shrink-0 rtl:-scale-x-100" />
          {trend.label}
        </dd>
      ) : null}
      {hint ? <dd className="text-sm text-ink-2">{hint}</dd> : null}
    </dl>
  );
}
