import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { ScanInput } from "@/components/scan-input";
import { useFindByCode } from "@/lib/ipc/products";
import { useStockLevels } from "@/lib/ipc/stock";
import { localizedName } from "@/lib/names";
import { formatQty, normalizeDigits } from "@/lib/numeric";
import { useLang } from "@/lib/use-language";

/** F4 (SAL-028): name, price and stock of a scanned or typed code; nothing is sold. */
export function PriceCheckDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const find = useFindByCode();
  const [code, setCode] = useState("");
  const product = find.data?.product ?? null;
  const stock = useStockLevels(product?.id ?? null);
  const onHand = (stock.data ?? []).filter((l) => l.kind === "storage").reduce((sum, l) => sum + l.qty, 0);
  const look = (typed: string) => {
    if (typed.trim() !== "") find.mutate(normalizeDigits(typed.trim()));
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    look(code);
  };
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())} title={t("pos:price.title")}>
      <form noValidate onSubmit={submit}>
        <Field id="price-code" label={t("pos:price.code")}>
          <ScanInput
            id="price-code"
            dir="auto"
            autoFocus
            value={code}
            onChange={(e) => setCode(e.target.value)}
            onScan={(scanned) => {
              setCode(scanned);
              look(scanned);
            }}
          />
        </Field>
      </form>
      {find.isSuccess && product === null ? (
        <p role="alert" className="text-danger">
          {t("pos:scan.notFound", { code })}
        </p>
      ) : null}
      {product ? (
        <dl className="grid gap-2 rounded-card border border-line p-4">
          <dt className="sr-only">{t("pos:price.name")}</dt>
          <dd className="text-2xl font-bold">{localizedName(product, lang)}</dd>
          <div className="flex justify-between text-xl">
            <dt>{t("pos:price.price")}</dt>
            <dd>
              <Amount value={product.sellPrice} className="font-bold" />
            </dd>
          </div>
          {product.trackStock ? (
            <div className="flex justify-between">
              <dt>{t("pos:price.stock")}</dt>
              <dd>
                <bdi dir="ltr">{formatQty(onHand, lang)}</bdi>
              </dd>
            </div>
          ) : null}
        </dl>
      ) : null}
    </Dialog>
  );
}
