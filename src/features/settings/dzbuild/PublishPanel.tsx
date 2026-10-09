import { Download, Search } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { formatDZD } from "@/lib/format";
import { useImportOnline, usePublishList, useReconcileStock, useSetPublish } from "@/lib/ipc/cloud";
import { useSetting } from "@/lib/ipc/settings";
import type { Session } from "@/lib/ipc/types";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** Which products the online store shows (DZB-010); beyond the plan's cap they stay drafts. */
export function PublishPanel({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [text, setText] = useState("");
  const [onlyPublished, setOnlyPublished] = useState(false);
  const search = useDebounced(text, 250).trim();
  const mode = useSetting("cloud.publish.mode");
  const allMode = mode.data === "all";
  const rows = usePublishList({ text: search || null, onlyPublished, limit: 500 }, mode.isFetched, mode.data);
  const set = useSetPublish();
  const reconcile = useReconcileStock();
  const importing = useImportOnline();
  const onError = (error: unknown) => toast.error(errorText(error));
  return (
    <Card>
      <h3 className="text-lg font-semibold">{t("sync:publish.title")}</h3>
      <p className="text-sm text-ink-2">{t("sync:publish.help")}</p>
      {allMode ? <p className="text-sm font-medium">{t("sync:publish.allMode")}</p> : null}
      <div className="flex flex-wrap items-center gap-3">
        <div className="relative grow">
          <Search aria-hidden className="pointer-events-none absolute start-3 top-3 size-5 text-ink-2" />
          <Input
            className="ps-10"
            aria-label={t("sync:publish.search")}
            placeholder={t("sync:publish.search")}
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        </div>
        <label className="flex min-h-11 items-center gap-2">
          <Checkbox checked={onlyPublished} onCheckedChange={(c) => setOnlyPublished(c === true)} />
          {t("sync:publish.onlyPublished")}
        </label>
      </div>
      {rows.error ? (
        <p role="alert" className="text-danger">
          {errorText(rows.error)}
        </p>
      ) : null}
      <ul className="flex max-h-96 flex-col overflow-y-auto">
        {rows.data?.map((r) => (
          <li key={r.productId} className="flex min-h-11 items-center gap-3 border-t border-line">
            <Checkbox
              aria-label={t("sync:publish.toggle", { name: r.name })}
              checked={r.publish}
              disabled={set.isPending}
              onCheckedChange={(c) =>
                set.mutate(
                  { productIds: [r.productId], publish: c === true, draft: r.onlineStatus === "draft" },
                  { onError },
                )
              }
            />
            <span className="grow">{r.name}</span>
            {r.onlineStatus === "draft" && r.publish ? (
              <span className="rounded-full bg-warn-soft px-2 text-sm">{t("sync:publish.draft")}</span>
            ) : null}
            {r.remoteId ? <span className="text-sm text-ok">{t("sync:publish.online")}</span> : null}
            <span className="text-end tabular-nums">
              <bdi dir="ltr">{formatDZD(r.price, lang)}</bdi>
            </span>
          </li>
        ))}
      </ul>
      <div className="flex flex-wrap gap-2">
        <Button
          variant="outline"
          disabled={!session.isOwner || importing.isPending}
          title={session.isOwner ? undefined : t("sync:link.ownerOnly")}
          onClick={() =>
            importing.mutate(undefined, {
              onSuccess: (r) =>
                toast.success(
                  t("sync:publish.imported", {
                    imported: r.imported,
                    matched: r.matched,
                    skipped: r.skippedVariants,
                  }),
                ),
              onError,
            })
          }
        >
          <Download aria-hidden />
          {t("sync:publish.import")}
        </Button>
        <Button
          variant="ghost"
          disabled={reconcile.isPending}
          onClick={() =>
            reconcile.mutate(undefined, {
              onSuccess: (n) => toast.success(t("sync:publish.reconciled", { count: n })),
              onError,
            })
          }
        >
          {t("sync:publish.reconcile")}
        </Button>
      </div>
    </Card>
  );
}
