import { BookOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import type { Hotkey } from "@/lib/hotkeys";
import { DEFAULT_KEYS, type TillAction, keyLabel } from "@/lib/key-map";

/** What each till action does (`pos:help.keys.*`). */
const ACTION_TEXT: Record<TillAction, string> = {
  help: "f1",
  search: "f2",
  customer: "f3",
  price_check: "f4",
  discount: "f5",
  hold: "f6",
  held: "f7",
  return: "f8",
  drawer: "f9",
  repeat_last: "f10",
  checkout: "f12",
  menu: "escape",
  floor: "floor",
  flexy: "flexy",
  gift_voucher: "giftVoucher",
  serial: "serial",
  department_keys: "departments",
};

/** The keys no one moves: the scale panel and the cart's own keys. */
const FIXED = ["f11", "plus", "minus", "multiply", "delete", "arrows"] as const;

/** The till's own actions (M1b), bound whatever the packs. */
const BASE_ACTIONS: TillAction[] = [
  "help",
  "search",
  "customer",
  "price_check",
  "discount",
  "hold",
  "held",
  "return",
  "drawer",
  "repeat_last",
  "checkout",
  "menu",
];

/** F1: the keyboard map (UX-010, UX-017) as this till binds it (its key map, the packs' actions
 * on), and the way to the help centre's page of the till. */
export function HelpDialog({
  keys = DEFAULT_KEYS,
  actions = BASE_ACTIONS,
  onClose,
}: {
  keys?: Record<TillAction, Hotkey>;
  actions?: readonly TillAction[];
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const rows = [
    ...actions.map((action) => ({ key: keyLabel(keys[action], t), text: ACTION_TEXT[action] })),
    ...FIXED.map((key) => ({ key: t(`common:keys.${key}`), text: key })),
  ];
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("pos:help.title")}
      footer={
        <Button
          variant="outline"
          onClick={() => {
            onClose();
            void navigate("/help/pos");
          }}
        >
          <BookOpen aria-hidden />
          {t("help:openFull")}
        </Button>
      }
    >
      <table className="w-full border-collapse">
        <tbody>
          {rows.map((row) => (
            <tr key={row.text} className="h-10 border-t border-line">
              <td className="w-24">
                <kbd dir="ltr" className="rounded-sm border border-line px-2 py-0.5 font-semibold">
                  {row.key}
                </kbd>
              </td>
              <td>{t(`pos:help.keys.${row.text}`)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Dialog>
  );
}
