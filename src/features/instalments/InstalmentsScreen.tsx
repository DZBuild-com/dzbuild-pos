import { ArrowLeft, Plus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, Route, Routes, useNavigate, useParams, useSearchParams } from "react-router";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { today } from "@/lib/dates";
import { useSession } from "@/lib/ipc/auth";
import type { ExportTarget, Permission } from "@/lib/ipc/types";
import { normalizeDigits } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { cn } from "@/lib/utils";
import { ContractsTable } from "./ContractsTable";
import { InstalmentWizard } from "./InstalmentWizard";
import { PlanPage } from "./PlanPage";
import { Reminders } from "./Reminders";

type Tab = "contracts" | "reminders";
const TABS: readonly Tab[] = ["contracts", "reminders"];

/** `/instalments` (TMP-001…009): the contracts and the reminders (`?tab=`), a new instalment sale
 * at `/instalments/new`, a contract at `/instalments/<id>`. */
export function InstalmentsScreen() {
  return (
    <Routes>
      <Route index element={<InstalmentsHome />} />
      <Route path="new" element={<NewContract />} />
      <Route path=":id" element={<PlanRoute />} />
    </Routes>
  );
}

function PlanRoute() {
  const { id } = useParams();
  return <PlanPage key={id} id={id ?? ""} />;
}

function InstalmentsHome() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const session = useSession();
  const can = (p: Permission) => session.data?.permissions.includes(p) ?? false;
  const [params, setParams] = useSearchParams();
  const tab = TABS.find((k) => k === params.get("tab")) ?? "contracts";
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-3xl font-bold">{t("instalments:title")}</h1>
        {can("sales.create") ? (
          <Button variant="primary" onClick={() => void navigate("new")}>
            <Plus aria-hidden />
            {t("instalments:new")}
          </Button>
        ) : null}
      </div>
      {/* the contracts and their dues are the kridi notebook's: `credit.view` */}
      {can("credit.view") ? (
        <>
          <div
            role="tablist"
            aria-label={t("instalments:tabs.label")}
            className="flex flex-wrap gap-2 border-b border-line pb-2"
          >
            {TABS.map((k) => (
              <button
                key={k}
                type="button"
                role="tab"
                id={`instalments-tab-${k}`}
                aria-selected={tab === k}
                aria-controls={`instalments-panel-${k}`}
                onClick={() => setParams({ tab: k }, { replace: true })}
                className={cn(
                  "min-h-11 rounded-control px-4 font-semibold",
                  tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
                )}
              >
                {t(`instalments:tabs.${k}`)}
              </button>
            ))}
          </div>
          <div role="tabpanel" id={`instalments-panel-${tab}`} aria-labelledby={`instalments-tab-${tab}`}>
            {tab === "contracts" ? <Contracts /> : <Reminders />}
          </div>
        </>
      ) : (
        <p className="text-ink-2">{t("instalments:noAccount")}</p>
      )}
    </div>
  );
}

/** The contracts, searched by number or customer, optionally the late ones only; the export is
 * the overdue instalments report of the day. */
function Contracts() {
  const { t } = useTranslation();
  const [search, setSearch] = useState("");
  const [overdueOnly, setOverdueOnly] = useState(false);
  const text = useDebounced(normalizeDigits(search.trim()), 200);
  const day = today();
  const exportTarget: ExportTarget = {
    kind: "report",
    request: { kind: "instalments_overdue", from: day, to: day },
  };
  return (
    <div className="flex flex-col gap-4">
      <TableToolbar
        search={search}
        onSearch={setSearch}
        searchHint={t("instalments:list.searchHint")}
        exportTarget={exportTarget}
        filters={
          <label className="flex min-h-11 items-center gap-3 font-semibold">
            <Checkbox checked={overdueOnly} onCheckedChange={(v) => setOverdueOnly(v === true)} />
            {t("instalments:plan.overdueOnly")}
          </label>
        }
      />
      <ContractsTable filter={{ text, overdueOnly }} />
    </div>
  );
}

/** A new instalment sale from the back office: the goods are picked in the wizard. */
function NewContract() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  return (
    <div className="flex max-w-5xl flex-col gap-6">
      <Link
        to="/instalments"
        className="inline-flex min-h-11 items-center gap-2 self-start font-semibold text-ink-2 hover:text-ink"
      >
        <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
        {t("instalments:title")}
      </Link>
      <h1 className="text-3xl font-bold">{t("instalments:new")}</h1>
      <InstalmentWizard onDone={(planId) => void navigate(`/instalments/${planId}`)} />
    </div>
  );
}
