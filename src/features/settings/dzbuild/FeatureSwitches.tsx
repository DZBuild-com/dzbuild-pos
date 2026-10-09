import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { NativeSelect } from "@/components/ui/native-select";
import { type SettingKey, useSetSetting, useSetting } from "@/lib/ipc/settings";
import type { JsonValue } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

const SWITCHES: readonly { key: SettingKey; label: string; note?: string }[] = [
  { key: "cloud.sync.enabled", label: "sync:features.enabled" },
  { key: "cloud.stock.push", label: "sync:features.stockPush" },
  { key: "cloud.stock.pull", label: "sync:features.stockPull" },
  // off by default: a price changed on DZBuild is told, the till's own price stays
  { key: "cloud.prices.pull", label: "sync:features.pricesPull", note: "sync:features.pricesPullNote" },
  { key: "cloud.images.push", label: "sync:features.images" },
  { key: "cloud.sales.push", label: "sync:features.sales" },
  { key: "cloud.orders.intake", label: "sync:features.orders" },
  { key: "cloud.customers.flags", label: "sync:features.flags", note: "sync:features.flagsNote" },
  // loi 18-07: off by default, the owner turns it on knowingly
  { key: "cloud.customers.sync", label: "sync:features.customers", note: "sync:features.customersNote" },
  { key: "cloud.receipt_qr", label: "sync:features.receiptQr" },
];

const RETENTION_DAYS = [30, 90, 180, 365] as const;

/** Saves a cloud setting (the engine wakes on it); a refusal is said, never swallowed. */
function useSave(): { save: (key: SettingKey, value: JsonValue) => void; pending: boolean } {
  const errorText = useErrorText();
  const set = useSetSetting();
  return {
    save: (key, value) => set.mutate({ key, value }, { onError: (error) => toast.error(errorText(error)) }),
    pending: set.isPending,
  };
}

function Switch({ setting, label, note }: { setting: SettingKey; label: string; note?: string }) {
  const { t } = useTranslation();
  const value = useSetting(setting);
  const { save, pending } = useSave();
  const id = `switch-${setting}`;
  return (
    <div className="flex items-start gap-3 py-2">
      <Checkbox
        id={id}
        checked={value.data === true}
        disabled={!value.isSuccess || pending}
        onCheckedChange={(checked) => save(setting, checked === true)}
        className="mt-0.5"
      />
      <div className="flex flex-col gap-0.5">
        <label htmlFor={id} className="font-medium">
          {t(label)}
        </label>
        {note ? <p className="text-sm text-ink-2">{t(note)}</p> : null}
      </div>
    </div>
  );
}

export function FeatureSwitches() {
  const { t } = useTranslation();
  const mode = useSetting("cloud.publish.mode");
  const orderDocument = useSetting("cloud.orders.document");
  const retention = useSetting("cloud.orders.retention_days");
  const { save, pending } = useSave();
  return (
    <Card>
      <h3 className="text-lg font-semibold">{t("sync:features.title")}</h3>
      <div className="flex flex-col">
        {SWITCHES.map((s) => (
          <Switch key={s.key} setting={s.key} label={s.label} note={s.note} />
        ))}
      </div>
      <div className="grid gap-4 md:grid-cols-3">
        <label className="flex flex-col gap-1.5">
          <span className="font-semibold">{t("sync:features.publishMode")}</span>
          <NativeSelect
            value={String(mode.data ?? "selected")}
            disabled={!mode.isSuccess || pending}
            onChange={(e) => save("cloud.publish.mode", e.target.value)}
          >
            <option value="selected">{t("sync:features.publishSelected")}</option>
            <option value="all">{t("sync:features.publishAll")}</option>
          </NativeSelect>
        </label>
        <label className="flex flex-col gap-1.5">
          <span className="font-semibold">{t("sync:features.orderDocument")}</span>
          <NativeSelect
            value={String(orderDocument.data ?? "ticket")}
            disabled={!orderDocument.isSuccess || pending}
            onChange={(e) => save("cloud.orders.document", e.target.value)}
          >
            <option value="ticket">{t("sync:features.orderTicket")}</option>
            <option value="delivery_note">{t("sync:features.orderDeliveryNote")}</option>
          </NativeSelect>
        </label>
        <label className="flex flex-col gap-1.5">
          <span className="font-semibold">{t("sync:features.retention")}</span>
          <NativeSelect
            value={String(retention.data ?? 90)}
            disabled={!retention.isSuccess || pending}
            onChange={(e) => save("cloud.orders.retention_days", Number(e.target.value))}
          >
            {RETENTION_DAYS.map((d) => (
              <option key={d} value={d}>
                {t("sync:features.days", { count: d })}
              </option>
            ))}
          </NativeSelect>
        </label>
      </div>
    </Card>
  );
}
