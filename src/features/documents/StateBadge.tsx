import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Badge } from "@/components/ui/badge";
import type { DocState } from "@/lib/ipc/types";

type Tone = "neutral" | "info" | "warn" | "ok" | "danger";

const TONES: Record<DocState["state"], Tone> = {
  draft: "neutral",
  cancelled: "neutral",
  open: "info",
  expired: "warn",
  converted: "ok",
  to_invoice: "warn",
  invoiced: "ok",
  unpaid: "warn",
  paid: "ok",
  partly_credited: "info",
  credited: "danger",
  final: "neutral",
};

/** Where the value goes in a translated state: the text is split around it, so the amount or the
 * numbers keep their own `<bdi dir="ltr">` inside the sentence. */
const SLOT = "[[value]]";

function around(text: string, value: ReactNode): ReactNode {
  const [before = "", after = ""] = text.split(SLOT);
  return (
    <>
      {before}
      {value}
      {after}
    </>
  );
}

/** A document's state as a pill (spec §10.4 tones): the amount left, credited or the credit notes
 * that cancelled it come from the backend, never computed here. */
export function StateBadge({ state }: { state: DocState }) {
  const { t } = useTranslation();
  let label: ReactNode;
  let tone = TONES[state.state];
  switch (state.state) {
    case "unpaid": {
      const key = state.overdue ? "billing:states.overdue" : "billing:states.unpaid";
      if (state.overdue) tone = "danger";
      label = around(t(key, { amount: SLOT }), <Amount value={state.remaining} />);
      break;
    }
    case "partly_credited":
      label = around(
        t("billing:states.partly_credited", { amount: SLOT }),
        <Amount value={state.credited} />,
      );
      break;
    case "credited":
      label = around(
        t("billing:states.credited", { numbers: SLOT }),
        <bdi dir="ltr">{state.by.join(", ")}</bdi>,
      );
      break;
    default:
      label = t(`billing:states.${state.state}`);
  }
  return (
    <Badge tone={tone} data-state={state.state}>
      {label}
    </Badge>
  );
}
