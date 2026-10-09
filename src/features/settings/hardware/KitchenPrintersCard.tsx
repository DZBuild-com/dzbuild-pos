import { useTranslation } from "react-i18next";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { NativeSelect } from "@/components/ui/native-select";
import { useStations } from "@/lib/ipc/restaurant";
import type { KitchenRouting, StationPrinter } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import type { CardProps } from "./HardwareHub";

/** pos-hw `KITCHEN_COPIES`: 1 to 3 copies of each kitchen ticket. */
const COPIES = [1, 2, 3] as const;

/** A station's route with another printer, copies or buzzer; `null` printer = no route (the
 * kitchen fallback, else the receipt printer, prints its tickets). */
export function withRoute(
  routing: KitchenRouting,
  stationId: string,
  change: {
    printerId?: string | null;
    copies?: number;
    buzzer?: boolean;
  },
): KitchenRouting {
  const stations = routing.stations ?? [];
  const current = stations.find((r) => r.stationId === stationId);
  const others = stations.filter((r) => r.stationId !== stationId);
  const printerId = change.printerId === undefined ? current?.printerId : change.printerId;
  if (!printerId) return { ...routing, stations: others };
  const route: StationPrinter = {
    stationId,
    printerId,
    copies: change.copies ?? current?.copies ?? 1,
    buzzer: change.buzzer ?? current?.buzzer ?? false,
  };
  return { ...routing, stations: [...others, route] };
}

/** Réglages › Matériel › Cuisine (RST-006, HW-004): which of this PC's printers prints each
 * preparation station's kitchen tickets, how many copies and with the buzzer; a station without a
 * printer prints on the kitchen fallback, else on the receipt printer. Each PC maps the shop's
 * stations to the printers it reaches (a network kitchen printer is shared). */
export function KitchenPrintersCard({ config, onChange, errorFor }: CardProps) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const stations = useStations();
  const printers = config.printers ?? [];
  const routing: KitchenRouting = config.kitchen ?? {};
  const setRouting = (next: KitchenRouting) => onChange({ ...config, kitchen: next });
  const live = (stations.data ?? []).filter((s) => !s.archived);
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("config:hardware.kitchen.title")}</h2>
      <p className="text-ink-2">{t("config:hardware.kitchen.hint")}</p>
      {stations.error ? (
        <p role="alert" className="text-danger">
          {errorText(stations.error)}
        </p>
      ) : null}
      {printers.length === 0 ? <p className="text-ink-2">{t("config:hardware.kitchen.noPrinter")}</p> : null}
      {stations.isSuccess && live.length === 0 ? (
        <p className="text-ink-2">{t("config:hardware.kitchen.noStation")}</p>
      ) : null}
      <ul className="flex flex-col gap-3">
        {live.map((station, i) => {
          const name = localizedName(station, lang);
          const route = routing.stations?.find((r) => r.stationId === station.id);
          const error = errorFor?.(`kitchen.stations.${i}.printerId`);
          return (
            <li key={station.id}>
              <fieldset className="flex flex-col gap-3 rounded-card border border-line p-3">
                <legend className="px-1 font-semibold">{name}</legend>
                <div className="grid gap-3 sm:grid-cols-3">
                  <div className="flex flex-col gap-1.5">
                    <label htmlFor={`kitchen-printer-${station.id}`} className="font-semibold">
                      {t("config:hardware.kitchen.printer")}
                    </label>
                    <NativeSelect
                      id={`kitchen-printer-${station.id}`}
                      value={route?.printerId ?? ""}
                      onChange={(e) =>
                        setRouting(withRoute(routing, station.id, { printerId: e.target.value || null }))
                      }
                    >
                      <option value="">{t("config:hardware.kitchen.default")}</option>
                      {printers.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                    </NativeSelect>
                    {error ? (
                      <p role="alert" className="text-danger">
                        {error}
                      </p>
                    ) : null}
                  </div>
                  <div className="flex flex-col gap-1.5">
                    <label htmlFor={`kitchen-copies-${station.id}`} className="font-semibold">
                      {t("config:hardware.kitchen.copies")}
                    </label>
                    <NativeSelect
                      id={`kitchen-copies-${station.id}`}
                      dir="ltr"
                      disabled={!route}
                      value={route?.copies ?? 1}
                      onChange={(e) =>
                        setRouting(withRoute(routing, station.id, { copies: Number(e.target.value) }))
                      }
                    >
                      {COPIES.map((n) => (
                        <option key={n} value={n}>
                          {n}
                        </option>
                      ))}
                    </NativeSelect>
                  </div>
                  <label className="flex min-h-11 items-center gap-3 self-end font-semibold">
                    <Checkbox
                      disabled={!route}
                      checked={route?.buzzer ?? false}
                      onCheckedChange={(v) =>
                        setRouting(withRoute(routing, station.id, { buzzer: v === true }))
                      }
                    />
                    {t("config:hardware.kitchen.buzzer")}
                  </label>
                </div>
              </fieldset>
            </li>
          );
        })}
      </ul>
      <div className="flex flex-col gap-1.5">
        <label htmlFor="kitchen-fallback" className="font-semibold">
          {t("config:hardware.kitchen.fallback")}
        </label>
        <NativeSelect
          id="kitchen-fallback"
          value={routing.fallbackPrinterId ?? ""}
          onChange={(e) =>
            setRouting({
              ...routing,
              fallbackPrinterId: e.target.value || null,
            })
          }
        >
          <option value="">{t("config:hardware.kitchen.receiptPrinter")}</option>
          {printers.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </NativeSelect>
      </div>
    </Card>
  );
}
