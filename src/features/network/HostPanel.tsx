import { Plus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type { HostStatus, Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { FirewallCard } from "./FirewallCard";
import { NoticesPanel } from "./NoticesPanel";
import { PairingDialog } from "./PairingDialog";
import { TillsTable } from "./TillsTable";

/** This PC's addresses as a till types them (`192.168.1.10:7345`). */
export function addressesOf(host: HostStatus): string[] {
  return host.addresses.map((ip) => `${ip}:${host.port}`);
}

/** The main PC's side (LAN-003/004/011): its server, its tills, their alerts, the firewall. */
export function HostPanel({ session, host }: { session: Session; host: HostStatus }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const [pairing, setPairing] = useState(false);
  const addresses = addressesOf(host);
  return (
    <>
      <Card>
        <h2 className="text-xl font-bold">{t("sync:lan.host.title")}</h2>
        {host.listening ? (
          addresses.length > 0 ? (
            <div className="flex flex-col gap-1">
              {addresses.map((address) => (
                <p key={address}>{t("sync:lan.host.listening", { address })}</p>
              ))}
              <p className="text-sm text-ink-2">{t("sync:lan.host.addressHelp")}</p>
            </div>
          ) : (
            <p className="text-warn">{t("sync:lan.host.noAddress")}</p>
          )
        ) : (
          <p role="alert" className="text-danger">
            {t("sync:lan.host.notListening", { reason: errorText({ code: host.error ?? "unknown" }) })}
          </p>
        )}
        <p>{t("sync:lan.host.tills", { online: host.tillsOnline, total: host.tillsTotal })}</p>
        <div>
          <Button variant="primary" disabled={!host.listening} onClick={() => setPairing(true)}>
            <Plus aria-hidden />
            {t("sync:lan.host.addTill")}
          </Button>
        </div>
      </Card>
      <TillsTable session={session} />
      <NoticesPanel />
      <FirewallCard />
      {pairing ? <PairingDialog addresses={addresses} onClose={() => setPairing(false)} /> : null}
    </>
  );
}
