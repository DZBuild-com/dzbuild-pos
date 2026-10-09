import { useTranslation } from "react-i18next";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { useOpenDrawer } from "@/lib/ipc/hardware";
import type { DrawerConfig, SerialKick } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import type { CardProps } from "./HardwareHub";
import { BaudSelect, PortSelect } from "./SerialFields";

type Via = DrawerConfig["kind"];

/** "Tiroir": the cash drawer through the receipt printer's RJ11 (M1b) or a USB/COM adapter
 * (HW-040/041). */
export function DrawerCard({ config, onChange, errorFor }: CardProps) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const open = useOpenDrawer();
  const drawer: DrawerConfig = config.drawer ?? { kind: "printer" };
  const setDrawer = (next: DrawerConfig) => onChange({ ...config, drawer: next });
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("hardware:drawer.title")}</h2>
      <RadioCards
        legend={t("hardware:drawer.via")}
        name="drawer-via"
        value={drawer.kind}
        onChange={(via: Via) =>
          setDrawer(
            via === "printer"
              ? { kind: "printer" }
              : { kind: "serial", port: "", baud: 9_600, kick: "esc_pos" },
          )
        }
        options={(["printer", "serial"] as const).map((via) => ({
          value: via,
          label: t(`hardware:drawer.vias.${via}`),
        }))}
      />
      {drawer.kind === "serial" ? (
        <div className="grid gap-3 sm:grid-cols-2">
          <PortSelect
            id="drawer-port"
            label={t("hardware:drawer.port")}
            none={t("hardware:printer.choose")}
            value={drawer.port}
            onChange={(port) => setDrawer({ ...drawer, port })}
            error={errorFor?.("drawer.port")}
          />
          <BaudSelect
            id="drawer-baud"
            label={t("hardware:printer.baud")}
            value={drawer.baud}
            onChange={(baud) => setDrawer({ ...drawer, baud })}
          />
          <Field id="drawer-kick" label={t("hardware:drawer.kick")}>
            <NativeSelect
              id="drawer-kick"
              value={drawer.kick}
              onChange={(e) => setDrawer({ ...drawer, kick: e.target.value as SerialKick })}
            >
              {(["esc_pos", "dtr"] as const).map((k) => (
                <option key={k} value={k}>
                  {t(`hardware:drawer.kicks.${k}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
        </div>
      ) : null}
      <div className="flex flex-col gap-2">
        <p className="text-sm text-ink-2">{t("hardware:drawer.savedFirst")}</p>
        <div>
          <Button
            variant="outline"
            disabled={open.isPending}
            onClick={() => open.mutate({ reason: "check", note: null })}
          >
            {t("hardware:drawer.test")}
          </Button>
        </div>
        {open.isSuccess ? <p className="text-ok">{t("hardware:drawer.sent")}</p> : null}
        {open.error ? (
          <p role="alert" className="text-danger">
            {errorText(open.error)}
          </p>
        ) : null}
      </div>
    </Card>
  );
}
