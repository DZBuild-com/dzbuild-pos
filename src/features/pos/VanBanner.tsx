import { Van } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { today } from "@/lib/dates";
import { useStockLocations } from "@/lib/ipc/stock";
import { useRoundsOut } from "@/lib/ipc/till-packs";
import type { Round } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { useTillKit } from "./till-kit";

/** The rounds a van can still be on: those started this past week. */
const ROUND_DAYS = 7;

/** VRT-101: the till sells from a van's stock (`SaleExtras.locationId`) until it goes back to the
 * shop. */
export function VanBanner() {
  const { t } = useTranslation();
  const { useExtras } = useTillKit();
  const van = useExtras((s) => s.van);
  if (!van) return null;
  return (
    <div
      role="status"
      className="flex items-center justify-between gap-3 bg-info-soft px-4 py-2 font-semibold text-info"
    >
      <span className="flex items-center gap-2">
        <Van aria-hidden className="rtl:-scale-x-100" />
        {t("packs:till.van.banner", { van: van.name, round: van.round })}
      </span>
      <Button variant="outline" onClick={() => useExtras.getState().setVan(null)}>
        {t("packs:till.van.back")}
      </Button>
    </div>
  );
}

/** "Vendre depuis le camion": one of the vans of a round on the road. */
export function VanPicker({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const { useExtras } = useTillKit();
  const [days] = useState(() => {
    const now = Date.now();
    return { from: today(now - ROUND_DAYS * 86_400_000), to: today(now) };
  });
  const rounds = useRoundsOut(days.from, days.to);
  const locations = useStockLocations();
  const vans = (rounds.data ?? []).filter(
    (r): r is Round & { vanLocationId: string } => r.vanLocationId !== null,
  );
  const vanName = (round: Round & { vanLocationId: string }) => {
    const location = locations.data?.find((l) => l.id === round.vanLocationId);
    return location ? localizedName(location, lang) : (round.vanLocationName ?? round.number);
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("packs:till.van.title")}
      description={t("packs:till.van.pick")}
    >
      {rounds.error ? (
        <p role="alert" className="text-danger">
          {errorText(rounds.error)}
        </p>
      ) : null}
      {rounds.isSuccess && vans.length === 0 ? (
        <p className="text-ink-2">{t("packs:till.van.none")}</p>
      ) : null}
      <div className="flex flex-col gap-2">
        {vans.map((round) => (
          <Button
            key={round.id}
            variant="outline"
            size="lg"
            className="justify-start"
            onClick={() => {
              useExtras.getState().setVan({
                locationId: round.vanLocationId,
                name: vanName(round),
                round: round.number,
              });
              onClose();
            }}
          >
            {t("packs:till.van.option", { van: vanName(round), round: round.number })}
          </Button>
        ))}
      </div>
    </Dialog>
  );
}
