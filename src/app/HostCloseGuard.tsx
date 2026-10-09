import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useCloseRequests, useConfirmClose } from "@/lib/ipc/lan";

/** LAN-003: closing the main PC while tills are online asks first (they keep selling offline). */
export function HostCloseGuard() {
  const { t } = useTranslation();
  const [asking, setAsking] = useState(false);
  const confirm = useConfirmClose();
  useCloseRequests(() => setAsking(true));
  return (
    <Dialog
      open={asking}
      onOpenChange={setAsking}
      title={t("sync:lan.shell.closeHostTitle")}
      description={t("sync:lan.shell.closeHostBody")}
      // its "Fermer" closes the app: the title row's close button (also "Fermer") would be ambiguous
      dismissible={false}
      footer={
        <>
          <Button variant="outline" onClick={() => setAsking(false)}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="danger" disabled={confirm.isPending} onClick={() => confirm.mutate()}>
            {t("sync:lan.shell.closeHostConfirm")}
          </Button>
        </>
      }
    >
      {null}
    </Dialog>
  );
}
