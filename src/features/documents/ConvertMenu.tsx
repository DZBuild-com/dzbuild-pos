import { ChevronDown } from "lucide-react";
import { DropdownMenu } from "radix-ui";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Button } from "@/components/ui/button";
import { useConvert } from "@/lib/ipc/billing";
import type { DocState, DocType, Document } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

type Target = "invoice" | "delivery_note" | "proforma";

/** ORD-001: what a validated quote or proforma becomes. */
const TARGETS: Partial<Record<DocType, readonly Target[]>> = {
  quote: ["invoice", "delivery_note", "proforma"],
  proforma: ["invoice", "delivery_note"],
};

const LABELS: Record<Target, { now: string; repriced: string }> = {
  invoice: { now: "billing:convert.toInvoice", repriced: "billing:convert.toInvoiceRepriced" },
  delivery_note: { now: "billing:convert.toDelivery", repriced: "billing:convert.toDeliveryRepriced" },
  proforma: { now: "billing:convert.toProforma", repriced: "billing:convert.toProformaRepriced" },
};

/**
 * "Transformer" on a validated quote (into an invoice, a delivery note or a proforma) or proforma
 * (into an invoice or a delivery note): the backend makes a draft at the offered prices, which
 * opens in the editor. An expired offer is taken again at today's prices only; one already
 * converted says into what. `allowed` is the kinds the user may make (every target by default;
 * the backend checks anyway).
 */
export function ConvertMenu({
  doc,
  state,
  allowed,
}: {
  doc: Document;
  state: DocState;
  allowed?: readonly DocType[];
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const navigate = useNavigate();
  const convert = useConvert();
  const targets = (TARGETS[doc.docType] ?? []).filter((k) => !allowed || allowed.includes(k));
  if (doc.status !== "validated" || targets.length === 0) return null;
  const expired = state.state === "expired";
  const converted = state.state === "converted";
  const run = (target: Target) =>
    convert.mutate(
      { sourceId: doc.id, target, reprice: expired },
      { onSuccess: (draft) => void navigate(`/documents/${draft.id}`) },
    );
  return (
    <div className="flex flex-col gap-1">
      <DropdownMenu.Root>
        <DropdownMenu.Trigger asChild>
          <Button variant="secondary" disabled={converted || convert.isPending}>
            {t("billing:convert.button")}
            <ChevronDown aria-hidden />
          </Button>
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content
            align="end"
            sideOffset={4}
            className="z-50 flex min-w-56 flex-col rounded-control border border-line bg-surface p-1 text-ink shadow-overlay"
          >
            {targets.map((target) => (
              <DropdownMenu.Item
                key={target}
                onSelect={() => run(target)}
                className="flex min-h-11 cursor-pointer items-center rounded-sm px-3 outline-none data-[highlighted]:bg-brand-soft"
              >
                {t(expired ? LABELS[target].repriced : LABELS[target].now)}
              </DropdownMenu.Item>
            ))}
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
      {state.state === "converted" ? (
        <p className="text-sm text-ink-2">
          {t("billing:convert.already", { numbers: state.into.join(", ") })}
        </p>
      ) : null}
      {convert.error ? (
        <p role="alert" className="text-sm text-danger">
          {errorText(convert.error)}
        </p>
      ) : null}
    </div>
  );
}
