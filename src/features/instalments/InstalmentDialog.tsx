import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { Dialog } from "@/components/ui/dialog";
import type { Cart, SignedDone, SignInput } from "@/lib/ipc/types";
import { InstalmentWizard, type WizardHandle } from "./InstalmentWizard";

/**
 * "Vente à tempérament" from the till's menu: the wizard over the till with its current cart (and
 * its customer). Once signed, the till empties its cart (`onSigned`); "Terminer" closes. Esc goes
 * back a step like on `/instalments/new` (what was typed stays); it closes from the first step
 * and once signed.
 *
 * Mounted lazily by the till's dialogs (`PosDialogs`, kind `instalment`), never in the till's
 * entry: the till hands over its cart rather than this chunk importing its store.
 */
export function InstalmentDialog({
  cart,
  extras,
  onSigned,
  onClose,
}: {
  cart: Cart;
  extras?: SignInput["extras"];
  onSigned: (done: SignedDone) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const wizard = useRef<WizardHandle>(null);
  return (
    <Dialog
      open
      size="xl"
      onOpenChange={(open) => (open ? undefined : onClose())}
      onEscapeKeyDown={(event) => {
        if (wizard.current?.escape()) event.preventDefault();
      }}
      title={t("instalments:sale")}
    >
      <InstalmentWizard
        ref={wizard}
        initialCart={cart}
        extras={extras}
        onSigned={onSigned}
        onDone={onClose}
      />
    </Dialog>
  );
}
