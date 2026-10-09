import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { NumericInput } from "@/components/numeric-input";
import { formatNumber } from "@/lib/format";
import { useLang } from "@/lib/use-language";
import { type Counts, DENOMINATIONS, countedTotal, isNote } from "./denominations";

/** Notes and coins counted one row per denomination (CSH-006). */
export function CountSheet({ counts, onChange }: { counts: Counts; onChange: (counts: Counts) => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  return (
    <div className="flex flex-col gap-2">
      <table className="w-full border-collapse">
        <tbody>
          {DENOMINATIONS.map((d) => {
            const value = formatNumber(d / 100, lang);
            const label = isNote(d) ? t("cash:count.note", { value }) : t("cash:count.coin", { value });
            const id = `count-${d}`;
            const count = Number(counts[d] || "0");
            return (
              <tr key={d} className="h-12 border-t border-line">
                <th scope="row" className="text-start font-semibold">
                  <label htmlFor={id}>{label}</label>
                </th>
                <td className="w-28 px-2">
                  <NumericInput
                    id={id}
                    kind="integer"
                    value={counts[d] ?? ""}
                    onValueChange={(v) => onChange({ ...counts, [d]: v })}
                  />
                </td>
                <td className="w-40 text-end">
                  <Amount value={d * count} />
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <p className="flex items-center justify-between text-xl font-bold">
        <span>{t("cash:count.total")}</span>
        <Amount value={countedTotal(counts)} />
      </p>
    </div>
  );
}
