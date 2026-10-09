import { formatDZD } from "@/lib/format";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** An amount in centimes, formatted for the UI language, isolated LTR inside Arabic text. */
export function Amount({
  value,
  className,
  hideCentimes,
}: {
  value: number;
  className?: string;
  hideCentimes?: boolean;
}) {
  const lang = useLang();
  return (
    <bdi dir="ltr" className={cn("tabular-nums", className)}>
      {formatDZD(value, lang, { hideCentimes })}
    </bdi>
  );
}
