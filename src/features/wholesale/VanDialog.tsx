import { X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { isApprovalCancelled, useApproval } from "@/components/manager-approval";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import {
  ProductPicker,
  SerialList,
  newLineKey,
  refusedLine,
  useLineLabel,
  useTracking,
  useUnitDecimals,
} from "@/features/purchases/purchase-lines";
import { toAppError } from "@/lib/ipc/client";
import type { TransferLine } from "@/lib/ipc/types";
import { useLoadVan } from "@/lib/ipc/wholesale";
import { formatQty, parseQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

interface LoadDraft {
  key: string;
  productId: string;
  name: string;
  unitCode: string;
  qty: string;
  serials: string[];
}

/**
 * VRT-101: loads the round's van from the shop — products by search or scan and their quantities,
 * the IMEIs of the serial-tracked ones — as one M2 transfer note from the default location to the
 * van (the backend takes the soonest-expiring lots first). The driver then sells from the van at
 * the till, and what is left comes back with "Décharger le camion".
 */
export function VanDialog({ roundId, van, onClose }: { roundId: string; van: string; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const label = useLineLabel();
  const decimalsOf = useUnitDecimals();
  const approval = useApproval();
  const load = useLoadVan();
  const [lines, setLines] = useState<LoadDraft[]>([]);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [failure, setFailure] = useState<{ field: string | null; message: string } | null>(null);
  const [sentKeys, setSentKeys] = useState<string[]>([]);
  const tracking = useTracking(lines.map((l) => l.productId));
  const refusedAt = refusedLine(failure?.field);
  const refusedKey = refusedAt !== null ? sentKeys[refusedAt] : undefined;
  const patch = (key: string, change: Partial<LoadDraft>) => {
    setLines((all) => all.map((l) => (l.key === key ? { ...l, ...change } : l)));
    setErrors((all) => ({ ...all, [key]: "" }));
  };

  const submit = () => {
    const found: Record<string, string> = {};
    const sent: TransferLine[] = [];
    for (const line of lines) {
      const serialTracked = tracking.of(line.productId)?.serials ?? false;
      const qty = parseQty(line.qty, decimalsOf(line.unitCode));
      if (qty === null) found[line.key] = t("validation:qtyFormat");
      else if (serialTracked && line.serials.length * 1_000 !== qty) {
        found[line.key] = t("stock:transfer.serialsCount", { qty: line.qty });
      } else {
        sent.push({
          productId: line.productId,
          qty,
          lotNumber: null,
          serials: serialTracked ? line.serials : [],
        });
      }
    }
    setErrors(found);
    setFailure(lines.length === 0 ? { field: "lines", message: t("wholesale:rounds.loadNothing") } : null);
    if (lines.length === 0 || Object.keys(found).length > 0) return;
    setSentKeys(lines.map((l) => l.key));
    approval
      .run(() => load.mutateAsync({ id: roundId, lines: sent }))
      .then((done) => {
        toast.success(t("wholesale:rounds.loaded", { number: done.document.number ?? "" }));
        onClose();
      })
      .catch((error: unknown) => {
        if (!isApprovalCancelled(error)) {
          setFailure({ field: toAppError(error).field ?? null, message: errorText(error) });
        }
      });
  };

  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("wholesale:rounds.loadTitle", { van })}
      description={t("wholesale:rounds.loadHint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={load.isPending || approval.pending || !tracking.ready}
            onClick={submit}
          >
            {t("wholesale:rounds.loadSubmit")}
          </Button>
        </>
      }
    >
      <form
        noValidate
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <ProductPicker
          autoFocus
          scanAnywhere
          onPick={(p) =>
            setLines((all) => {
              // the same product scanned again adds to its line
              const same = all.find((l) => l.productId === p.productId);
              if (same) {
                const qty = parseQty(same.qty, decimalsOf(same.unitCode));
                if (qty === null) return all;
                return all.map((l) =>
                  l === same ? { ...l, qty: formatQty(qty + (p.packQty ?? 1_000), lang) } : l,
                );
              }
              const line: LoadDraft = {
                key: newLineKey(),
                productId: p.productId,
                name: p.name,
                unitCode: p.unitCode,
                qty: formatQty(p.packQty ?? 1_000, lang),
                serials: [],
              };
              return [...all, line];
            })
          }
        />
        <section aria-label={t("stock:transfer.lines")} className="flex flex-col gap-3">
          {lines.map((line) => (
            <div key={line.key} className="flex flex-col gap-3 rounded-card border border-line p-3">
              <div className="flex items-center justify-between gap-2">
                <p className="font-semibold" dir="auto">
                  {line.name}
                </p>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t("purchases:lines.removeOf", { name: line.name })}
                  onClick={() => setLines((all) => all.filter((l) => l.key !== line.key))}
                >
                  <X aria-hidden />
                </Button>
              </div>
              <Field
                id={`van-qty-${line.key}`}
                label={t("wholesale:rounds.qty")}
                error={errors[line.key] || undefined}
              >
                <NumericInput
                  id={`van-qty-${line.key}`}
                  kind="qty"
                  className="w-40"
                  aria-label={label("wholesale:rounds.qty", line.name)}
                  aria-invalid={Boolean(errors[line.key])}
                  value={line.qty}
                  onValueChange={(qty) => patch(line.key, { qty })}
                />
              </Field>
              {tracking.of(line.productId)?.serials ? (
                <SerialList
                  id={`van-serials-${line.key}`}
                  label={label("stock:transfer.serials", line.name)}
                  serials={line.serials}
                  onChange={(serials) => patch(line.key, { serials })}
                />
              ) : null}
              {refusedKey === line.key && failure ? (
                <p role="alert" className="text-sm text-danger">
                  {failure.message}
                </p>
              ) : null}
            </div>
          ))}
        </section>
        {failure && refusedKey === undefined ? (
          <p role="alert" className="text-danger">
            {failure.message}
          </p>
        ) : null}
      </form>
      {approval.dialog}
    </Dialog>
  );
}
