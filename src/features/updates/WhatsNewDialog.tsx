import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useLang } from "@/lib/use-language";
import { loadReleaseNotes } from "./release-notes";

/** "What's new" of `version`: its bundled notes in the UI language (French when that list is
 * empty), opened after an update and from the Updates screen. */
export function WhatsNewDialog({ version, onClose }: { version: string; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const notes = useQuery({
    queryKey: ["release-notes", version],
    queryFn: () => loadReleaseNotes(version),
    staleTime: Infinity,
  });
  // The notes are a small chunk loaded on demand: the dialog opens with them.
  if (notes.isPending) return null;
  const items = notes.data ? (notes.data[lang].length > 0 ? notes.data[lang] : notes.data.fr) : [];
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("common:whatsNew.title")}
      footer={
        <Button variant="outline" onClick={onClose}>
          {t("common:actions.close")}
        </Button>
      }
    >
      <p className="text-ink-2">
        {t("common:appName")} <bdi dir="ltr">{version}</bdi>
      </p>
      <ul className="flex list-disc flex-col gap-2 ps-6">
        {items.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>
    </Dialog>
  );
}
