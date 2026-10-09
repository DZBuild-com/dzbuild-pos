import { Undo2 } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { SupplierSelect } from "@/features/purchases/purchase-lines";
import { SupplierReturnDialog } from "@/features/purchases/SupplierReturnDialog";
import { formatDate } from "@/lib/format";
import { useProduct } from "@/lib/ipc/products";
import { usePurchase } from "@/lib/ipc/purchases";
import { useSerials } from "@/lib/ipc/stock-m2";
import { useSupplier } from "@/lib/ipc/suppliers";
import type { Serial, SerialStatus } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useDebounced } from "@/lib/use-debounced";
import { useLang } from "@/lib/use-language";
import { useCan, useLocations } from "./stock-parts";

const STATUSES: readonly SerialStatus[] = ["in_stock", "sold", "returned", "returned_to_supplier"];
const TONES: Record<SerialStatus, "ok" | "info" | "warn" | "neutral"> = {
  in_stock: "ok",
  sold: "info",
  returned: "warn",
  returned_to_supplier: "neutral",
};

/** The product of a serial, in the UI language (read once per product and cached). */
function ProductName({ id }: { id: string }) {
  const lang = useLang();
  const product = useProduct(id).data;
  return product ? localizedName(product, lang) : null;
}

const col = columnHelper<Serial>();

/** PRD-050: the serial register (IMEI for phones) searched by number, with each piece's product,
 * status, location and sale link; a piece in stock goes back to the supplier through a supplier
 * return (`BR…-F-…`), which moves the stock and the serial together. */
export function SerialsTab() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const { nameOf } = useLocations();
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState<SerialStatus | "">("");
  const [sending, setSending] = useState<Serial | null>(null);
  const text = useDebounced(search.trim(), 200);
  const serials = useSerials({ text, status: status === "" ? null : status, limit: 500 });
  const returns = can("purchases.manage");

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("serialNo", {
          header: () => t("stock:serials.columns.serial"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("productId", {
          header: () => t("stock:serials.columns.product"),
          enableSorting: false,
          cell: (c) => <ProductName id={c.getValue()} />,
        }),
        col.accessor("status", {
          header: () => t("stock:serials.columns.status"),
          cell: (c) => <Badge tone={TONES[c.getValue()]}>{t(`stock:serials.status.${c.getValue()}`)}</Badge>,
        }),
        col.accessor((s) => (s.locationId ? nameOf(s.locationId) : ""), {
          id: "location",
          header: () => t("stock:serials.columns.location"),
        }),
        col.accessor((s) => s.saleLineId !== null, {
          id: "sale",
          header: () => t("stock:serials.columns.sale"),
          cell: (c) => t(c.getValue() ? "stock:serials.linked" : "stock:serials.notLinked"),
        }),
        col.accessor("updatedAt", {
          header: () => t("stock:serials.columns.updatedAt"),
          cell: (c) => <bdi dir="ltr">{formatDate(c.getValue(), lang, { withTime: true })}</bdi>,
        }),
        ...(returns
          ? [
              col.display({
                id: "action",
                header: () => t("stock:serials.columns.action"),
                cell: (c) =>
                  c.row.original.status === "in_stock" ? (
                    <Button
                      variant="outline"
                      className="min-h-9"
                      aria-label={t("stock:serials.sendBackOf", { serial: c.row.original.serialNo })}
                      onClick={(event) => {
                        event.stopPropagation();
                        setSending(c.row.original);
                      }}
                    >
                      <Undo2 aria-hidden className="rtl:-scale-x-100" />
                      {t("stock:serials.sendBack")}
                    </Button>
                  ) : null,
              }),
            ]
          : []),
      ]),
    [t, lang, nameOf, returns],
  );

  return (
    <div className="flex flex-col gap-4">
      <TableToolbar
        search={search}
        onSearch={setSearch}
        onScan={setSearch}
        exportTarget={null}
        filters={
          <Field id="serials-status" label={t("stock:fields.status")}>
            <NativeSelect
              id="serials-status"
              value={status}
              onChange={(event) => setStatus(STATUSES.find((s) => s === event.target.value) ?? "")}
            >
              <option value="">{t("stock:fields.allStatuses")}</option>
              {STATUSES.map((s) => (
                <option key={s} value={s}>
                  {t(`stock:serials.status.${s}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
        }
      />
      <DataTable
        ariaLabel={t("stock:serials.list")}
        data={serials.data ?? []}
        columns={columns}
        getRowId={(s) => s.id}
        empty={serials.isPending ? t("common:status.loading") : t("stock:serials.empty")}
      />
      {sending ? <SendBackDialog serial={sending} onClose={() => setSending(null)} /> : null}
    </div>
  );
}

/** Asks which supplier takes the piece back (the one of its goods receipt by default), then opens
 * the supplier return with that serial number on its line. */
function SendBackDialog({ serial, onClose }: { serial: Serial; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const product = useProduct(serial.productId).data;
  const receipt = usePurchase(can("purchases.view") ? serial.receivedDocumentId : null).data;
  const [choice, setChoice] = useState<string | null>(null);
  const [chosen, setChosen] = useState<string | null>(null);
  const [missing, setMissing] = useState(false);
  const supplier = useSupplier(chosen).data;
  const value = choice ?? receipt?.supplierId ?? "";

  if (supplier && product) {
    return (
      <SupplierReturnDialog
        supplier={supplier}
        open
        onClose={onClose}
        initial={{
          productId: product.id,
          name: localizedName(product, lang),
          unitCode: product.unitCode,
          serials: [serial.serialNo],
        }}
      />
    );
  }
  return (
    <Dialog
      open
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("purchases:chooseSupplier.title")}
      description={t("stock:serials.sendBackOf", { serial: serial.serialNo })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={chosen !== null || !product}
            onClick={() => {
              setMissing(value === "");
              if (value !== "") setChosen(value);
            }}
          >
            {t("common:actions.next")}
          </Button>
        </>
      }
    >
      <SupplierSelect
        id="send-back-supplier"
        value={value}
        onChange={(id) => {
          setChoice(id);
          setMissing(false);
        }}
        allLabel={t("purchases:chooseSupplier.placeholder")}
        error={missing ? t("validation:required") : undefined}
      />
    </Dialog>
  );
}
