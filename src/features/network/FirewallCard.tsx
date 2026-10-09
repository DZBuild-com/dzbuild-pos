import { Copy, RefreshCw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useFirewall } from "@/lib/ipc/lan";
import type { FirewallCheck, RuleState } from "@/lib/ipc/types";

interface Rule {
  state: RuleState;
  /** Which rule, by its port: an administrator finds it in Windows Defender Firewall. */
  labelKey: string;
  /** What the shop loses while it is missing or disabled. */
  missingKey: string;
  command: string | undefined;
}

/** The two rules M6's installer adds, in `pos_server::firewall::install_commands`' order: the
 * tills' connections (TCP 7345), then the main PC's announce on the network (mDNS). */
function rulesOf(check: FirewallCheck): Rule[] {
  return [
    {
      state: check.status.tcpRule,
      labelKey: "sync:lan.firewall.ruleTcp",
      missingKey: "sync:lan.firewall.missing",
      command: check.commands[0],
    },
    {
      state: check.status.mdnsRule,
      labelKey: "sync:lan.firewall.ruleMdns",
      missingKey: "sync:lan.firewall.missingMdns",
      command: check.commands[1],
    },
  ];
}

/** LAN-008: can the tills reach this PC through Windows Defender Firewall? Each rule that is not
 * there says what it blocks and comes with the `netsh` line that adds it. */
export function FirewallCard() {
  const { t } = useTranslation();
  const firewall = useFirewall(true);
  const [copied, setCopied] = useState<string | null>(null);
  const check = firewall.data;
  if (!check) return null;
  const { status } = check;
  const rules = rulesOf(check);
  const missing = rules.filter((rule) => rule.state === "missing" || rule.state === "disabled");
  const toAdd = rules.filter((rule) => rule.state !== "present");
  const publicNetworks = status.networks.filter((network) => network.category === "public");
  return (
    <Card>
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-xl font-bold">{t("sync:lan.firewall.title")}</h2>
        {status.ok ? <Badge tone="ok">{t("sync:lan.firewall.ok")}</Badge> : null}
      </div>
      {rules.some((rule) => rule.state === "unknown") ? (
        <p className="text-ink-2">{t("sync:lan.firewall.unknown")}</p>
      ) : null}
      {missing.map((rule) => (
        <div key={rule.labelKey} role="alert" className="flex flex-col gap-0.5">
          <p className="text-danger">{t(rule.missingKey)}</p>
          <p className="text-sm text-ink-2">{t(rule.labelKey)}</p>
        </div>
      ))}
      {publicNetworks.map((network) => (
        <p key={network.alias} role="alert" className="text-warn">
          {t("sync:lan.firewall.public", { name: network.alias })}
        </p>
      ))}
      {toAdd.length > 0 ? (
        <div className="flex flex-col gap-2">
          <p className="text-sm text-ink-2">{t("sync:lan.firewall.commandsHelp")}</p>
          {toAdd.map(({ labelKey, command }) =>
            command ? (
              <div key={labelKey} className="flex items-start gap-2">
                <pre
                  dir="ltr"
                  className="min-w-0 flex-1 overflow-x-auto rounded-control bg-surface-2 p-2 text-sm whitespace-pre-wrap"
                >
                  {command}
                </pre>
                <Button
                  variant="outline"
                  onClick={() => {
                    // Copying may be refused (no clipboard access): the line stays selectable.
                    void navigator.clipboard?.writeText(command).then(
                      () => setCopied(labelKey),
                      () => setCopied(null),
                    );
                  }}
                >
                  <Copy aria-hidden />
                  {copied === labelKey ? t("sync:lan.firewall.copied") : t("sync:lan.firewall.copy")}
                </Button>
              </div>
            ) : null,
          )}
        </div>
      ) : null}
      <div>
        <Button variant="outline" disabled={firewall.isFetching} onClick={() => void firewall.refetch()}>
          <RefreshCw aria-hidden />
          {t("sync:lan.firewall.recheck")}
        </Button>
      </div>
    </Card>
  );
}
