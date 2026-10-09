import { Plus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Amount } from "@/components/amount";
import { Splash } from "@/components/splash";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { dayToMs, today } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useCtrlHotkey } from "@/lib/hotkeys";
import { useServiceBoard } from "@/lib/ipc/services";
import type { ServiceKind, ServiceSummary } from "@/lib/ipc/types";
import { normalizeDigits } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { IntakeDialog } from "./IntakeDialog";
import { OPEN_STATUSES, SERVICE_KINDS } from "./status";

/**
 * ORD-005/006: the tickets still in the shop, one column per status (Reçu → Prêt), filtered by
 * kind and by a text (number, item, IMEI, customer); read again every 30 s. A card opens its
 * ticket; "Nouveau dépôt" (Ctrl+N) takes a new item in.
 */
export function ServiceBoard() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const [kind, setKind] = useState<ServiceKind | "">("");
  const [text, setText] = useState("");
  const [intake, setIntake] = useState(false);
  const query = useDebounced(normalizeDigits(text.trim()), 200);
  const board = useServiceBoard({ status: null, kind: kind === "" ? null : kind, text: query });
  useCtrlHotkey("n", () => setIntake(true), !intake);
  const day = today();

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end gap-3">
        <Field id="service-kind" label={t("services:board.kind")}>
          <NativeSelect
            id="service-kind"
            className="w-auto"
            value={kind}
            onChange={(event) => setKind(SERVICE_KINDS.find((k) => k === event.target.value) ?? "")}
          >
            <option value="">{t("services:board.allKinds")}</option>
            {SERVICE_KINDS.map((k) => (
              <option key={k} value={k}>
                {t(`services:kinds.${k}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <div className="min-w-56 flex-1">
          <Field id="service-search" label={t("services:board.search")} hint={t("services:board.searchHint")}>
            <Input
              id="service-search"
              type="search"
              dir="auto"
              autoComplete="off"
              value={text}
              onChange={(event) => setText(event.target.value)}
            />
          </Field>
        </div>
        <Button
          variant="primary"
          className="mb-7"
          onClick={() => setIntake(true)}
          shortcut="Control+N"
          shortcutLabel={t("common:keys.ctrlN")}
        >
          <Plus aria-hidden />
          {t("services:board.new")}
        </Button>
      </div>
      {board.error ? (
        <p role="alert" className="text-danger">
          {errorText(board.error)}
        </p>
      ) : null}
      {board.isPending ? <Splash inline /> : null}
      {/* five columns side by side, once the tickets are read; on a narrow window they scroll inside
          their frame */}
      {board.data ? (
        <div className="overflow-x-auto">
          <div
            aria-label={t("services:board.label")}
            className="grid min-w-[52rem] grid-cols-5 gap-3"
            role="group"
          >
            {OPEN_STATUSES.map((status) => {
              const cards = (board.data ?? []).filter((r) => r.status === status);
              const label = t(`services:status.${status}`);
              return (
                <section
                  key={status}
                  aria-label={label}
                  className="flex min-w-0 flex-col gap-2 rounded-card bg-surface-2 p-3"
                >
                  <h2 className="flex items-center justify-between gap-2 font-bold">
                    <span>{label}</span>
                    <Badge>{cards.length}</Badge>
                  </h2>
                  {cards.length === 0 ? (
                    <p className="text-sm text-ink-2">{t("services:board.empty")}</p>
                  ) : (
                    cards.map((card) => (
                      <TicketCard
                        key={card.id}
                        card={card}
                        late={card.status !== "ready" && card.promisedOn !== null && card.promisedOn < day}
                        onOpen={() => void navigate(`/services/${card.id}`)}
                      />
                    ))
                  )}
                </section>
              );
            })}
          </div>
        </div>
      ) : null}
      {intake ? <IntakeDialog onClose={() => setIntake(false)} /> : null}
    </div>
  );
}

function TicketCard({ card, late, onOpen }: { card: ServiceSummary; late: boolean; onOpen: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  return (
    <button
      type="button"
      onClick={onOpen}
      className={cn(
        "flex min-h-11 flex-col gap-1 rounded-card border-2 bg-surface p-3 text-start hover:bg-brand-soft focus-visible:outline-3 focus-visible:outline-focus",
        late ? "border-warn-soft" : "border-line",
      )}
    >
      <span className="flex items-center justify-between gap-2">
        <bdi dir="ltr" className="font-bold tabular-nums">
          {card.number}
        </bdi>
        <span className="text-sm text-ink-2">{t("services:days", { count: card.daysOpen })}</span>
      </span>
      <span className="font-semibold">{card.item}</span>
      <span className="text-sm">{card.customerName}</span>
      {card.technicianName ? <span className="text-sm text-ink-2">{card.technicianName}</span> : null}
      {card.promisedOn ? (
        <span className="text-sm text-ink-2">
          {t("services:board.promised", { date: formatDate(dayToMs(card.promisedOn), lang) })}
        </span>
      ) : null}
      {late ? <Badge tone="warn">{t("services:board.late")}</Badge> : null}
      {card.deposits > 0 ? (
        <span className="text-sm">
          {t("services:uncollected.deposit")} <Amount value={card.deposits} className="font-semibold" />
        </span>
      ) : null}
    </button>
  );
}
