import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { useClearTare, useScaleProbe, useTare, useWeight, useZero } from "@/lib/ipc/devices";
import type {
  DataBits,
  Parity,
  ScaleConfig,
  ScaleProtocol,
  SerialSettings,
  WeightEvent,
} from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { DeviceDot } from "./DeviceDot";
import type { CardProps } from "./HardwareHub";
import { BaudSelect, eightN1, PortSelect } from "./SerialFields";

const PROTOCOLS: readonly ScaleProtocol[] = ["toledo8217", "cas_ecr", "continuous"];
const PARITIES: readonly Parity[] = ["none", "even", "odd"];

/** pos-hw `ScaleProtocol::serial`: Toledo 8217 at 9600 7E1, the others 9600 8N1. */
function protocolSerial(protocol: ScaleProtocol, port: string): SerialSettings {
  return protocol === "toledo8217"
    ? { port, baud: 9_600, dataBits: "seven", parity: "even", stopBits: "one" }
    : eightN1(port);
}

/** pos-hw `ScaleConfig::new`: polled every 250 ms, stable after 3 equal readings, 30 kg. */
function newScale(protocol: ScaleProtocol, port: string): ScaleConfig {
  return {
    serial: protocolSerial(protocol, port),
    protocol,
    pollMs: 250,
    stableReadings: 3,
    capacityG: 30_000,
  };
}

/** "Balance": the checkout scale on a serial port (VRT-010, HW-030). */
export function ScaleCard({ config, onChange, errorFor }: CardProps) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const probe = useScaleProbe();
  const scale = config.scale ?? null;
  const port = scale?.serial.port ?? "";
  const setScale = (next: ScaleConfig | null) => onChange({ ...config, scale: next });
  const setSerial = (change: Partial<SerialSettings>) => {
    if (scale) setScale({ ...scale, serial: { ...scale.serial, ...change } });
  };
  const choosePort = (next: string) => {
    probe.reset();
    if (next === "") setScale(null);
    else
      setScale(scale ? { ...scale, serial: { ...scale.serial, port: next } } : newScale("toledo8217", next));
  };
  const detect = () =>
    probe.mutate(port, {
      onSuccess: (found) => {
        // a scale already set up keeps its polling, stability and capacity settings
        if (found)
          setScale({
            ...(scale ?? newScale(found.protocol, port)),
            protocol: found.protocol,
            serial: found.serial,
          });
      },
    });

  return (
    <Card>
      <div className="flex items-center gap-3">
        <h2 className="text-xl font-bold">{t("hardware:scale.title")}</h2>
        <DeviceDot device="scale" />
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <PortSelect
          id="scale-port"
          label={t("hardware:scale.port")}
          none={t("hardware:scale.none")}
          value={port}
          onChange={choosePort}
          error={errorFor?.("scale.serial.port")}
        />
        <div className="flex items-end">
          <Button variant="outline" disabled={port === "" || probe.isPending} onClick={detect}>
            {t("hardware:scale.detect")}
          </Button>
        </div>
      </div>
      {probe.data === null ? (
        <p role="alert" className="text-warn">
          {t("hardware:scale.notFound")}
        </p>
      ) : null}
      {probe.error ? (
        <p role="alert" className="text-danger">
          {errorText(probe.error)}
        </p>
      ) : null}
      {scale ? (
        <>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field id="scale-protocol" label={t("hardware:scale.protocol")}>
              <NativeSelect
                id="scale-protocol"
                value={scale.protocol}
                onChange={(e) => {
                  const protocol = e.target.value as ScaleProtocol;
                  setScale({ ...scale, protocol, serial: protocolSerial(protocol, port) });
                }}
              >
                {PROTOCOLS.map((p) => (
                  <option key={p} value={p}>
                    {t(`hardware:scale.protocols.${p}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <BaudSelect
              id="scale-baud"
              label={t("hardware:scale.baud")}
              value={scale.serial.baud}
              onChange={(baud) => setSerial({ baud })}
            />
            <Field id="scale-bits" label={t("hardware:scale.bits")}>
              <NativeSelect
                id="scale-bits"
                dir="ltr"
                value={scale.serial.dataBits}
                onChange={(e) => setSerial({ dataBits: e.target.value as DataBits })}
              >
                <option value="seven">7</option>
                <option value="eight">8</option>
              </NativeSelect>
            </Field>
            <Field id="scale-parity" label={t("hardware:scale.parity")}>
              <NativeSelect
                id="scale-parity"
                value={scale.serial.parity}
                onChange={(e) => setSerial({ parity: e.target.value as Parity })}
              >
                {PARITIES.map((p) => (
                  <option key={p} value={p}>
                    {t(`hardware:scale.parities.${p}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
            <Field id="scale-poll" label={t("hardware:scale.pollMs")} error={errorFor?.("scale.pollMs")}>
              <NumericInput
                id="scale-poll"
                kind="integer"
                value={String(scale.pollMs)}
                onValueChange={(v) => setScale({ ...scale, pollMs: Math.min(2_000, Number(v || "0")) })}
              />
            </Field>
            <Field
              id="scale-stable"
              label={t("hardware:scale.stableReadings")}
              error={errorFor?.("scale.stableReadings")}
            >
              <NumericInput
                id="scale-stable"
                kind="integer"
                value={String(scale.stableReadings)}
                onValueChange={(v) => setScale({ ...scale, stableReadings: Math.min(10, Number(v || "0")) })}
              />
            </Field>
            <Field
              id="scale-capacity"
              label={t("hardware:scale.capacityG")}
              error={errorFor?.("scale.capacityG")}
            >
              <NumericInput
                id="scale-capacity"
                kind="integer"
                value={String(scale.capacityG)}
                onValueChange={(v) => setScale({ ...scale, capacityG: Math.min(300_000, Number(v || "0")) })}
              />
            </Field>
          </div>
          <LiveWeight />
        </>
      ) : null}
    </Card>
  );
}

const TONES = {
  ok: "ok",
  motion: "warn",
  underload: "warn",
  overload: "danger",
  disconnected: "danger",
  no_response: "danger",
} as const;

/** What the scale says right now, with its stable badge: a weight is only added when stable. */
function stateOf(reading: WeightEvent): keyof typeof TONES {
  return reading.state === "ok" && !reading.stable ? "motion" : reading.state;
}

/** The saved scale's live reading and its keys; commands to a scale that cannot do them (Zéro on
 * a continuous scale) come back as their translated refusal. */
function LiveWeight() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const reading = useWeight(true);
  const tare = useTare();
  const clearTare = useClearTare();
  const zero = useZero();
  const failed = tare.error ?? clearTare.error ?? zero.error;
  // one message at a time: the last key pressed
  const press = (key: typeof tare | typeof clearTare | typeof zero) => {
    for (const other of [tare, clearTare, zero]) if (other !== key) other.reset();
    key.mutate();
  };
  const state = reading ? stateOf(reading) : null;
  return (
    <div className="flex flex-col gap-3 rounded-card border border-line p-4">
      <div data-testid="live-weight" className="flex flex-wrap items-center gap-3">
        <span className="font-semibold">{t("hardware:scale.live")}</span>
        {reading && state ? (
          <>
            <bdi dir="ltr" className="text-2xl font-bold tabular-nums">
              {t("hardware:scale.net", { qty: formatQty(reading.net, lang) })}
            </bdi>
            <Badge tone={TONES[state]}>{t(`hardware:scale.states.${state}`)}</Badge>
          </>
        ) : (
          <span className="text-ink-2">{t("hardware:scale.waiting")}</span>
        )}
      </div>
      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" disabled={tare.isPending} onClick={() => press(tare)}>
          {t("hardware:scale.tare")}
        </Button>
        <Button variant="secondary" disabled={clearTare.isPending} onClick={() => press(clearTare)}>
          {t("hardware:scale.clearTare")}
        </Button>
        <Button variant="secondary" disabled={zero.isPending} onClick={() => press(zero)}>
          {t("hardware:scale.zero")}
        </Button>
      </div>
      {failed ? (
        <p role="alert" className="text-danger">
          {errorText(failed)}
        </p>
      ) : null}
    </div>
  );
}
