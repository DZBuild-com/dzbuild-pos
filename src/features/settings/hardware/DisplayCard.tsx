import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { useCloseDisplay, useMonitors, useOpenDisplayPreview, useTestVfd } from "@/lib/ipc/devices";
import type { Lang } from "@/lib/i18n";
import type { CustomerScreenConfig, VfdCharset, VfdCommands, VfdConfig } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";
import { DeviceDot } from "./DeviceDot";
import type { CardProps } from "./HardwareHub";
import { BaudSelect, eightN1, PortSelect } from "./SerialFields";

/** pos-hw `CustomerScreenConfig::default`. */
const SCREEN_DEFAULTS: Required<CustomerScreenConfig> = {
  enabled: false,
  monitor: null,
  showLogo: true,
  thanksSeconds: 5,
};

/** pos-hw `VfdConfig::new`: 2 × 20 ESC/POS, French words, no accents. */
function newVfd(port: string): VfdConfig {
  return {
    serial: eightN1(port),
    commands: "esc_pos",
    columns: 20,
    rows: 2,
    latinLang: "fr",
    charset: "ascii",
    codePageSelector: 19,
  };
}

/** "Afficheurs": the pole display (HW-043, Latin text only) and the second-screen customer display
 * (fully trilingual). */
export function DisplayCard({ config, onChange, errorFor }: CardProps) {
  const vfd = config.display?.vfd ?? null;
  const screen = { ...SCREEN_DEFAULTS, ...config.display?.screen };
  const setVfd = (next: VfdConfig | null) =>
    onChange({ ...config, display: { ...config.display, vfd: next } });
  const setScreen = (change: Partial<CustomerScreenConfig>) =>
    onChange({ ...config, display: { ...config.display, screen: { ...screen, ...change } } });
  return (
    <>
      <VfdSection vfd={vfd} onChange={setVfd} errorFor={errorFor} />
      <ScreenSection screen={screen} onChange={setScreen} errorFor={errorFor} />
    </>
  );
}

function VfdSection({
  vfd,
  onChange,
  errorFor,
}: {
  vfd: VfdConfig | null;
  onChange: (vfd: VfdConfig | null) => void;
  errorFor: CardProps["errorFor"];
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const test = useTestVfd();
  const set = (change: Partial<VfdConfig>) => {
    if (vfd) onChange({ ...vfd, ...change });
  };
  return (
    <Card>
      <div className="flex items-center gap-3">
        <h2 className="text-xl font-bold">{t("hardware:display.vfd.title")}</h2>
        <DeviceDot device="vfd" />
      </div>
      <p className="text-ink-2">{t("hardware:display.vfd.limit")}</p>
      <label className="flex min-h-11 items-center gap-3 font-semibold">
        <Checkbox checked={vfd !== null} onCheckedChange={(v) => onChange(v === true ? newVfd("") : null)} />
        {t("hardware:display.vfd.enabled")}
      </label>
      {vfd ? (
        <>
          <div className="grid gap-3 sm:grid-cols-2">
            <PortSelect
              id="vfd-port"
              label={t("hardware:printer.serialPort")}
              none={t("hardware:printer.choose")}
              value={vfd.serial.port}
              onChange={(port) => set({ serial: { ...vfd.serial, port } })}
              error={errorFor?.("display.vfd.serial.port")}
            />
            <BaudSelect
              id="vfd-baud"
              label={t("hardware:printer.baud")}
              value={vfd.serial.baud}
              onChange={(baud) => set({ serial: { ...vfd.serial, baud } })}
            />
            <Field id="vfd-commands" label={t("hardware:display.vfd.commands")}>
              <NativeSelect
                id="vfd-commands"
                value={vfd.commands}
                onChange={(e) => set({ commands: e.target.value as VfdCommands })}
              >
                {(["esc_pos", "cd5220"] as const).map((c) => (
                  <option key={c} value={c}>
                    {t(`hardware:display.vfd.commandSets.${c}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field
              id="vfd-columns"
              label={t("hardware:display.vfd.columns")}
              error={errorFor?.("display.vfd.columns")}
            >
              <NumericInput
                id="vfd-columns"
                kind="integer"
                value={String(vfd.columns)}
                onValueChange={(v) => set({ columns: Math.min(40, Number(v || "0")) })}
              />
            </Field>
            <Field id="vfd-rows" label={t("hardware:display.vfd.rows")}>
              <NativeSelect
                id="vfd-rows"
                dir="ltr"
                value={vfd.rows}
                onChange={(e) => set({ rows: Number(e.target.value) })}
              >
                <option value={1}>1</option>
                <option value={2}>2</option>
              </NativeSelect>
            </Field>
            <Field id="vfd-lang" label={t("hardware:display.vfd.latinLang")}>
              <NativeSelect
                id="vfd-lang"
                value={vfd.latinLang}
                onChange={(e) => set({ latinLang: e.target.value as Lang })}
              >
                <option value="fr">{t("common:languages.fr")}</option>
                <option value="en">{t("common:languages.en")}</option>
              </NativeSelect>
            </Field>
            <Field id="vfd-charset" label={t("hardware:display.vfd.charset")}>
              <NativeSelect
                id="vfd-charset"
                value={vfd.charset}
                onChange={(e) => set({ charset: e.target.value as VfdCharset })}
              >
                {(["ascii", "pc858"] as const).map((c) => (
                  <option key={c} value={c}>
                    {t(`hardware:display.vfd.charsets.${c}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </div>
          <div className="flex flex-col gap-2">
            <div>
              <Button variant="outline" disabled={test.isPending} onClick={() => test.mutate(vfd)}>
                {t("hardware:display.vfd.test")}
              </Button>
            </div>
            {test.isSuccess ? <p className="text-ok">{t("hardware:display.vfd.tested")}</p> : null}
            {test.error ? (
              <p role="alert" className="text-danger">
                {errorText(test.error)}
              </p>
            ) : null}
          </div>
        </>
      ) : null}
    </Card>
  );
}

function ScreenSection({
  screen,
  onChange,
  errorFor,
}: {
  screen: Required<CustomerScreenConfig>;
  onChange: (change: Partial<CustomerScreenConfig>) => void;
  errorFor: CardProps["errorFor"];
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const monitors = useMonitors(screen.enabled);
  const preview = useOpenDisplayPreview();
  const close = useCloseDisplay();
  const list = monitors.data ?? [];
  // `null`: the first monitor that is not the till's own (what the backend picks)
  const chosen = screen.monitor ?? list.find((m) => !m.mainWindow)?.name ?? null;
  const failed = preview.error ?? close.error ?? monitors.error;
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("hardware:display.screen.title")}</h2>
      <label className="flex min-h-11 items-center gap-3 font-semibold">
        <Checkbox checked={screen.enabled} onCheckedChange={(v) => onChange({ enabled: v === true })} />
        {t("hardware:display.screen.enabled")}
      </label>
      {screen.enabled ? (
        <>
          <fieldset className="flex flex-col gap-2">
            <legend className="mb-1 font-semibold">{t("hardware:display.screen.monitor")}</legend>
            <div className="grid gap-2 sm:grid-cols-2">
              {list.map((m) => (
                <label
                  key={m.name}
                  className={cn(
                    "flex min-h-11 items-center gap-2 rounded-card border-2 p-3 font-semibold",
                    chosen === m.name ? "border-brand bg-brand-soft" : "border-line bg-surface",
                    m.mainWindow && "text-ink-2",
                  )}
                >
                  <input
                    type="radio"
                    name="customer-monitor"
                    className="size-5 accent-brand"
                    disabled={m.mainWindow}
                    checked={chosen === m.name}
                    onChange={() => onChange({ monitor: m.name })}
                  />
                  <bdi dir="ltr">
                    {t("hardware:display.screen.monitorOption", {
                      name: m.name,
                      width: m.width,
                      height: m.height,
                    })}
                  </bdi>
                  {m.mainWindow ? <span>{t("hardware:display.screen.mainWindow")}</span> : null}
                </label>
              ))}
            </div>
          </fieldset>
          {monitors.isSuccess && list.length === 1 ? (
            <p className="text-ink-2">{t("hardware:display.screen.single")}</p>
          ) : null}
          <label className="flex min-h-11 items-center gap-3 font-semibold">
            <Checkbox checked={screen.showLogo} onCheckedChange={(v) => onChange({ showLogo: v === true })} />
            {t("hardware:display.screen.logo")}
          </label>
          <Field
            id="screen-thanks"
            label={t("hardware:display.screen.thanks")}
            error={errorFor?.("display.screen.thanksSeconds")}
          >
            <NumericInput
              id="screen-thanks"
              kind="integer"
              value={String(screen.thanksSeconds)}
              onValueChange={(v) => onChange({ thanksSeconds: Math.min(30, Number(v || "0")) })}
            />
          </Field>
        </>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <Button variant="outline" disabled={preview.isPending} onClick={() => preview.mutate()}>
          {t("hardware:display.screen.preview")}
        </Button>
        <Button variant="ghost" disabled={close.isPending} onClick={() => close.mutate()}>
          {t("hardware:display.screen.closePreview")}
        </Button>
      </div>
      {failed ? (
        <p role="alert" className="text-danger">
          {errorText(failed)}
        </p>
      ) : null}
    </Card>
  );
}
