import { keepPreviousData, useQuery } from "@tanstack/react-query";
import type { Lang } from "@/lib/i18n";
import { commands } from "./client";
import { qk } from "./keys";
import type { ReportRequest } from "./types";

/** Every report kind with its title in `lang`, its filters and whether the user may run it. The
 * previous language's catalogue stays while another loads: an open report keeps its filters. */
export function useReportCatalog(lang: Lang) {
  return useQuery({
    queryKey: qk.reportCatalog(lang),
    queryFn: () => commands.reportsCatalog(lang),
    placeholderData: keepPreviousData,
  });
}

/** One report with its labels; `null` waits (a required filter not chosen yet). */
export function useReport(request: ReportRequest | null) {
  return useQuery({
    queryKey: qk.report(request),
    queryFn: () => commands.reportsRun(request as ReportRequest),
    enabled: request !== null,
    placeholderData: keepPreviousData,
  });
}

/** Today's figures; what the user may not see comes back `null`. */
export function useDashboard(enabled = true) {
  return useQuery({ queryKey: qk.dashboard, queryFn: () => commands.reportsDashboard(), enabled });
}
