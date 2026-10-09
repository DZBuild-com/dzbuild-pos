import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { formatDate, formatDZD } from "@/lib/format";
import { useMarkSeen, useOnlineOrders } from "@/lib/ipc/online-orders";
import type { OrderFilter, Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { OrderDetail, paymentLabel } from "./OrderDetail";

const FILTERS: readonly OrderFilter[] = ["open", "mine", "done"];

/** The DZBuild store's orders on the main PC (DZB-014): the list by state, the chosen order
 * beside it. Opening an order takes it out of the rail's unseen count. */
export function OrdersScreen({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [filter, setFilter] = useState<OrderFilter>("open");
  const [selected, setSelected] = useState<string | null>(null);
  const orders = useOnlineOrders(filter);
  const { mutate: markSeen } = useMarkSeen();
  const unseen = orders.data?.some((o) => o.id === selected && !o.seen) ?? false;
  useEffect(() => {
    if (selected && unseen) markSeen([selected]);
  }, [selected, unseen, markSeen]);
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-3xl font-bold">{t("nav:orders")}</h1>
      <div className="flex items-start gap-4">
        <section className="flex w-80 shrink-0 flex-col gap-2" aria-label={t("sync:orders.list")}>
          <div role="tablist" className="flex flex-wrap gap-1">
            {FILTERS.map((f) => (
              <Button
                key={f}
                role="tab"
                aria-selected={filter === f}
                variant={filter === f ? "secondary" : "ghost"}
                onClick={() => setFilter(f)}
              >
                {t(`sync:orders.filters.${f}`)}
              </Button>
            ))}
          </div>
          {orders.error ? (
            <p role="alert" className="text-danger">
              {errorText(orders.error)}
            </p>
          ) : null}
          {orders.data?.length === 0 ? <p className="text-ink-2">{t("sync:orders.empty")}</p> : null}
          <ul className="flex flex-col gap-1">
            {orders.data?.map((o) => (
              <li key={o.id}>
                <button
                  type="button"
                  aria-current={selected === o.id ? "true" : undefined}
                  onClick={() => setSelected(o.id)}
                  className={cn(
                    "flex min-h-14 w-full items-center gap-3 rounded-control border border-line bg-surface p-3 text-start hover:bg-surface-2",
                    selected === o.id && "border-brand bg-brand-soft hover:bg-brand-soft",
                    !o.seen && "font-semibold",
                  )}
                >
                  <span className="min-w-0 grow">
                    <bdi dir="ltr">#{o.orderNumber}</bdi> · {o.customerName ?? t("sync:orders.noName")}
                    <span className="block text-sm font-normal text-ink-2">
                      {t(`sync:orders.states.${o.localState}`)} · {paymentLabel(t, o.paymentMethod)}
                      {o.claimedElsewhere ? ` · ${t("sync:orders.elsewhere")}` : ""} ·{" "}
                      <bdi dir="ltr">{formatDate(o.remoteCreatedAt, lang, { withTime: true })}</bdi>
                    </span>
                  </span>
                  <bdi dir="ltr" className="tabular-nums">
                    {formatDZD(o.total, lang)}
                  </bdi>
                </button>
              </li>
            ))}
          </ul>
        </section>
        {selected ? (
          <OrderDetail id={selected} session={session} />
        ) : (
          <p className="self-center text-ink-2">{t("sync:orders.pick")}</p>
        )}
      </div>
    </div>
  );
}
