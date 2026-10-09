import { Search } from "lucide-react";
import { type ReactNode, useId } from "react";
import { useTranslation } from "react-i18next";
import { GuardedInput } from "@/components/numeric-input";
import { ScanInput } from "@/components/scan-input";
import { Input } from "@/components/ui/input";
import type { ExportTarget } from "@/lib/ipc/types";
import { ExportMenu } from "./export-menu";

/** The row above every list (spec §10.4): search, the screen's own filters, then the exports.
 * `exportTarget: null` leaves the exports out (the user may not export this list). With `onScan`
 * the search takes the barcode scanner (a barcode, an IMEI): the code is decoded from the physical
 * keys, whatever the Windows layout typed. With `guardScans` a scan never lands in the search (the
 * screen's own window scanner reads it, like the count sheet). `searchId` names the search field
 * (a screen's `/` shortcut focuses it); `searchHint`, its placeholder, says what it finds (the
 * label above stays). */
export function TableToolbar({
  search,
  onSearch,
  onScan,
  guardScans = false,
  filters,
  exportTarget,
  onPdf,
  searchId,
  searchHint,
}: {
  search: string;
  onSearch: (search: string) => void;
  onScan?: (code: string) => void;
  guardScans?: boolean;
  filters?: ReactNode;
  exportTarget: ExportTarget | null;
  onPdf?: () => void;
  searchId?: string;
  searchHint?: string;
}) {
  const { t } = useTranslation();
  const autoId = useId();
  const id = searchId ?? autoId;
  const field = {
    id,
    type: "search",
    dir: "auto",
    className: "ps-10",
    placeholder: searchHint,
  } as const;
  return (
    <div className="flex flex-wrap items-end gap-3">
      <div className="flex min-w-60 flex-1 flex-col gap-1.5">
        <label htmlFor={id} className="font-semibold">
          {t("tables:search")}
        </label>
        <div className="relative">
          <Search
            aria-hidden
            className="pointer-events-none absolute start-3 top-1/2 size-5 -translate-y-1/2 text-ink-2"
          />
          {onScan ? (
            <ScanInput
              {...field}
              value={search}
              onChange={(event) => onSearch(event.target.value)}
              onScan={onScan}
            />
          ) : guardScans ? (
            <GuardedInput {...field} value={search} onValueChange={onSearch} />
          ) : (
            <Input
              {...field}
              autoComplete="off"
              value={search}
              onChange={(event) => onSearch(event.target.value)}
            />
          )}
        </div>
      </div>
      {filters}
      {exportTarget ? (
        <div className="ms-auto">
          <ExportMenu target={exportTarget} onPdf={onPdf} />
        </div>
      ) : null}
    </div>
  );
}
