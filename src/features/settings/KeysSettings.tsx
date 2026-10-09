import { Keyboard } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { useTillLink } from "@/lib/ipc/lan";
import { useSetTillKeyMap, useTillKeyMap } from "@/lib/ipc/privacy";
import { DEFAULT_KEYS, hotkeyOf, type TillAction } from "@/lib/key-map";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";

type KeyMap = Record<TillAction, string>;

const ACTIONS = Object.keys(DEFAULT_KEYS) as TillAction[];

function isAction(value: string): value is TillAction {
  return value in DEFAULT_KEYS;
}

/** UX-017: each action of the sale screen with its key; "Changer" takes the next allowed key,
 * refused while another action has it; only the keys that differ from the defaults are saved
 * (the backend checks the whole map again, `key_map_invalid`). The main PC's setting: read-only
 * on a till. */
export function KeysSettings() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const stored = useTillKeyMap();
  const save = useSetTillKeyMap();
  const till = useTillLink();
  const [draft, setDraft] = useState<Partial<KeyMap>>({});
  const [capturing, setCapturing] = useState<TillAction | null>(null);
  const [taken, setTaken] = useState<{ key: string; action: TillAction } | null>(null);

  const current = useMemo(() => {
    const map: KeyMap = { ...DEFAULT_KEYS };
    for (const [action, key] of stored.data ?? []) {
      if (isAction(action)) map[action] = key;
    }
    return map;
  }, [stored.data]);
  const keys = useMemo<KeyMap>(() => ({ ...current, ...draft }), [current, draft]);
  const label = (key: string) => (key === "Escape" ? t("common:keys.escape") : key);
  const actionName = (action: TillAction) => t(`config:keys.actions.${action}`);

  useEffect(() => {
    if (capturing === null) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const key = hotkeyOf(event);
      if (key === null) return;
      event.preventDefault();
      // the key is being chosen, not used: F1 must not open the help, F5 not reload the window
      event.stopImmediatePropagation();
      const owner = ACTIONS.find((a) => a !== capturing && keys[a] === key);
      if (owner) {
        setTaken({ key, action: owner });
        return;
      }
      setTaken(null);
      setDraft((previous) => ({ ...previous, [capturing]: key }));
      setCapturing(null);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [capturing, keys]);

  const changed = ACTIONS.some((a) => keys[a] !== current[a]);
  const send = (map: Record<string, string>, done: string) =>
    save.mutate(map, {
      onSuccess: () => {
        setDraft({});
        setCapturing(null);
        setTaken(null);
        toast.success(done);
      },
    });

  return (
    <Card aria-labelledby="keys-title">
      <h2 id="keys-title" className="text-xl font-bold">
        {t("config:keys.title")}
      </h2>
      <p className="text-ink-2">{t("config:keys.intro", { escape: t("common:keys.escape") })}</p>
      <HostOnlyNote />
      {stored.error ? (
        <p role="alert" className="text-danger">
          {errorText(stored.error)}
        </p>
      ) : null}
      <table className="w-full border-collapse">
        <thead className="text-sm text-ink-2">
          <tr className="h-10">
            <th scope="col" className="text-start font-semibold">
              {t("config:keys.action")}
            </th>
            <th scope="col" className="text-start font-semibold">
              {t("config:keys.key")}
            </th>
            <th scope="col">
              <span className="sr-only">{t("config:keys.change")}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {ACTIONS.map((action) => (
            <tr
              key={action}
              className={cn(
                "h-12 border-t border-line",
                capturing === action ? "bg-brand-soft" : "even:bg-surface-2",
              )}
            >
              <th scope="row" className="text-start font-semibold">
                {actionName(action)}
              </th>
              <td>
                {capturing === action ? (
                  <span role="status" className="text-ink-2">
                    {t("config:keys.press")}
                  </span>
                ) : (
                  <kbd
                    dir="ltr"
                    className={cn(
                      "rounded-sm border border-line bg-surface px-2 py-1 font-semibold tabular-nums",
                      keys[action] !== current[action] && "border-brand",
                    )}
                  >
                    {label(keys[action])}
                  </kbd>
                )}
              </td>
              <td className="text-end">
                {till.isTill ? null : capturing === action ? (
                  <Button
                    variant="ghost"
                    onClick={() => {
                      setCapturing(null);
                      setTaken(null);
                    }}
                  >
                    {t("common:actions.cancel")}
                  </Button>
                ) : (
                  <Button
                    variant="secondary"
                    aria-label={t("config:keys.changeFor", { action: actionName(action) })}
                    onClick={() => {
                      setTaken(null);
                      setCapturing(action);
                    }}
                  >
                    <Keyboard aria-hidden />
                    {t("config:keys.change")}
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {taken ? (
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {t("config:keys.taken", { key: label(taken.key), action: actionName(taken.action) })}
        </p>
      ) : null}
      {save.error ? (
        <p role="alert" className="text-danger">
          {errorText(save.error)}
        </p>
      ) : null}
      {till.isTill ? null : (
        <div className="flex flex-wrap justify-end gap-2">
          <Button
            variant="outline"
            disabled={save.isPending}
            onClick={() => send({}, t("config:keys.resetDone"))}
          >
            {t("config:keys.reset")}
          </Button>
          <Button
            variant="primary"
            disabled={save.isPending || !changed || capturing !== null}
            onClick={() =>
              send(
                Object.fromEntries(
                  ACTIONS.filter((a) => keys[a] !== DEFAULT_KEYS[a]).map((a) => [a, keys[a]]),
                ),
                t("config:keys.saved"),
              )
            }
          >
            {t("config:keys.save")}
          </Button>
        </div>
      )}
    </Card>
  );
}
