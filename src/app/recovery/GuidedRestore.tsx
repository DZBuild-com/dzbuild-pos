import { FolderOpen } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useEffect, useReducer, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Avatar } from "@/components/avatar";
import { PIN_MIN, PinInput } from "@/components/pin-pad";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { PreviewDetails } from "@/features/settings/backups/PreviewDetails";
import { formatDate, formatNumber } from "@/lib/format";
import { useOpenFolder } from "@/lib/ipc/app";
import { toAppError } from "@/lib/ipc/client";
import {
  useRecoveryBackups,
  useRecoveryPreview,
  useRecoveryRestore,
  useRecoveryStartFresh,
} from "@/lib/ipc/recovery";
import type { GuidedRestore as RestoreDone, RecoveryBackup } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** A date or a path inside a sentence, isolated left-to-right (the text form of
 * `<bdi dir="ltr">`, for a value interpolated into a translation). */
function ltr(text: string): string {
  return `\u2066${text}\u2069`;
}

/** Where the damaged files went, for support: shown once the app is about to restart. */
export function DamagedFolder({ path }: { path: string }) {
  const { t } = useTranslation();
  if (!path) return null;
  return (
    <p className="flex flex-col gap-1">
      <span className="font-semibold">{t("common:recovery.keptIn")}</span>
      <bdi dir="ltr" className="rounded-control bg-surface-2 p-2 font-mono text-sm break-all">
        {path}
      </bdi>
    </p>
  );
}

/** A keyboard-operable radio group (WAI-ARIA): one tab stop on the checked row, the arrows move
 * and choose (left and right follow the reading direction), Home and End jump; 56 px rows. */
function RadioRows<T extends { id: string }>({
  labelledBy,
  items,
  checked,
  onCheck,
  children,
}: {
  labelledBy: string;
  items: T[];
  checked: string | null;
  onCheck: (id: string) => void;
  children: (item: T) => ReactNode;
}) {
  const { i18n } = useTranslation();
  const rows = useRef(new Map<string, HTMLButtonElement>());
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = items.findIndex((item) => item.id === checked);
    const forward = i18n.dir() === "rtl" ? "ArrowLeft" : "ArrowRight";
    const backward = forward === "ArrowLeft" ? "ArrowRight" : "ArrowLeft";
    let next: number;
    if (event.key === "ArrowDown" || event.key === forward) next = (index + 1) % items.length;
    else if (event.key === "ArrowUp" || event.key === backward)
      next = (Math.max(index, 0) - 1 + items.length) % items.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = items.length - 1;
    else return;
    const item = items[next];
    if (!item) return;
    event.preventDefault();
    onCheck(item.id);
    rows.current.get(item.id)?.focus();
  };
  return (
    <div role="radiogroup" aria-labelledby={labelledBy} onKeyDown={onKeyDown} className="flex flex-col gap-2">
      {items.map((item, index) => {
        const on = item.id === checked;
        return (
          <button
            key={item.id}
            ref={(row) => {
              if (row) rows.current.set(item.id, row);
              else rows.current.delete(item.id);
            }}
            type="button"
            role="radio"
            aria-checked={on}
            tabIndex={on || (checked === null && index === 0) ? 0 : -1}
            onClick={() => onCheck(item.id)}
            className={cn(
              "flex min-h-14 items-center gap-3 rounded-control border-2 px-4 py-2 text-start",
              on ? "border-brand bg-brand-soft" : "border-line bg-surface hover:bg-surface-2",
            )}
          >
            {children(item)}
          </button>
        );
      })}
    </div>
  );
}

function Footer({ children }: { children: ReactNode }) {
  return <div className="flex flex-wrap justify-end gap-3">{children}</div>;
}

interface State {
  step: "choose" | "check" | "owner" | "done";
  /** The backup chosen; `null` until the owner moves off the newest one. */
  id: string | null;
  /** When the backup being restored was made (the restart message names it). */
  at: number;
  ownerId: string | null;
  pin: string;
  done: RestoreDone | null;
}

type Action =
  | { type: "pick"; id: string }
  | { type: "check"; backup: RecoveryBackup }
  | { type: "choose" }
  | { type: "owner" }
  | { type: "ownerId"; ownerId: string }
  | { type: "pin"; pin: string }
  | { type: "done"; done: RestoreDone };

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "pick":
      return { ...state, id: action.id };
    case "check":
      return { ...state, step: "check", id: action.backup.id, at: action.backup.at, ownerId: null, pin: "" };
    case "choose":
      return { ...state, step: "choose", pin: "" };
    case "owner":
      return { ...state, step: "owner" };
    case "ownerId":
      return { ...state, ownerId: action.ownerId };
    case "pin":
      return { ...state, pin: action.pin };
    case "done":
      return { ...state, step: "done", pin: "", done: action.done };
  }
}

/** Step 1: every backup found (this PC, the second copy, the USB sticks), newest first and
 * preselected; the list follows a stick plugged in or pulled out. */
function Choose({
  chosen,
  onPick,
  onCheck,
  onCancel,
}: {
  chosen: string | null;
  onPick: (id: string) => void;
  onCheck: (backup: RecoveryBackup) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const backups = useRecoveryBackups(true);
  const openFolder = useOpenFolder();
  const list = backups.data ?? [];
  const selected = list.find((b) => b.id === chosen) ?? list[0] ?? null;
  return (
    <>
      {backups.isPending ? <Splash inline /> : null}
      {backups.error ? (
        <p role="alert" className="text-danger">
          {errorText(backups.error)}
        </p>
      ) : null}
      {backups.isSuccess && list.length === 0 ? (
        <>
          <p>{t("common:recovery.none")}</p>
          <Button variant="outline" onClick={() => openFolder.mutate("data")}>
            <FolderOpen aria-hidden />
            {t("common:fatal.openFolder")}
          </Button>
        </>
      ) : null}
      {list.length > 0 ? (
        <>
          <h2 id="recovery-found" className="font-semibold">
            {t("common:recovery.found")}
          </h2>
          <RadioRows labelledBy="recovery-found" items={list} checked={selected?.id ?? null} onCheck={onPick}>
            {(b) => (
              <>
                <span className="flex min-w-0 flex-1 flex-col items-start">
                  <bdi dir="ltr" className="font-semibold">
                    {formatDate(b.at, lang, { withTime: true })}
                  </bdi>
                  <span className="text-sm text-ink-2">
                    {t(`settings:backups.kinds.${b.kind}`)} ·{" "}
                    {t(`common:recovery.places.${b.location}`, { place: ltr(b.place ?? "") })}
                  </span>
                </span>
                <bdi dir="ltr" className="text-sm text-ink-2">
                  {t("common:units.sizeMb", { value: formatNumber(b.size / 1_048_576, lang, 1) })}
                </bdi>
              </>
            )}
          </RadioRows>
        </>
      ) : null}
      <Footer>
        <Button variant="outline" onClick={onCancel}>
          {t("common:actions.cancel")}
        </Button>
        <Button variant="primary" disabled={!selected} onClick={() => selected && onCheck(selected)}>
          {t("common:recovery.check")}
        </Button>
      </Footer>
    </>
  );
}

/** Step 2: what the backup holds, read without changing it; only a sound backup of this version
 * or an older one goes further. */
function Check({ id, onBack, onContinue }: { id: string; onBack: () => void; onContinue: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const preview = useRecoveryPreview(id);
  const status = preview.data?.status;
  return (
    <>
      {preview.isPending ? <Splash inline /> : null}
      {preview.error ? (
        <p role="alert" className="text-danger">
          {errorText(preview.error)}
        </p>
      ) : null}
      {preview.data?.status === "valid" ? <PreviewDetails preview={preview.data} /> : null}
      {status === "invalid" ? (
        <p role="alert" className="text-danger">
          {t("common:recovery.invalid")}
        </p>
      ) : null}
      {status === "newer_than_app" ? (
        <p role="alert" className="text-danger">
          {t("settings:backups.preview.newer")}
        </p>
      ) : null}
      <Footer>
        <Button variant="outline" onClick={onBack}>
          {t("common:recovery.other")}
        </Button>
        {status === "valid" ? (
          <Button variant="primary" onClick={onContinue}>
            {t("common:recovery.continue")}
          </Button>
        ) : null}
      </Footer>
    </>
  );
}

/**
 * The fatal screen's guided restore (DAT-010 when the database cannot open): choose a backup,
 * check what it holds, then its owner's PIN — checked against the backup's own users, since the
 * live database cannot be read — moves the damaged files aside (never deleted), schedules the
 * restore and restarts the app on it.
 */
export function GuidedRestore({
  onCancel,
  allowFresh = false,
}: {
  onCancel: () => void;
  /** The database is gone (not damaged): the owner may start an empty shop anyway. */
  allowFresh?: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [state, dispatch] = useReducer(reducer, {
    step: "choose",
    id: null,
    at: 0,
    ownerId: null,
    pin: "",
    done: null,
  });
  const preview = useRecoveryPreview(state.step === "owner" ? state.id : null);
  const restore = useRecoveryRestore();
  const fresh = useRecoveryStartFresh();
  const [confirmFresh, setConfirmFresh] = useState(false);
  const [freshDone, setFreshDone] = useState(false);
  const section = useRef<HTMLElement>(null);
  // Each step replaces the buttons that led to it: keyboard and screen-reader users land on it.
  useEffect(() => section.current?.focus(), [state.step]);

  const owners = preview.data?.owners ?? [];
  const ownerId = state.ownerId ?? (owners.length === 1 ? (owners[0]?.id ?? null) : null);
  const canRestore =
    state.id !== null &&
    ownerId !== null &&
    state.pin.length >= PIN_MIN &&
    !restore.isPending &&
    !fresh.isPending;
  const failed = restore.error ?? fresh.error;
  const restoreError = failed
    ? toAppError(failed).code === "too_many_attempts"
      ? t("common:recovery.locked")
      : errorText(failed)
    : null;

  return (
    <section ref={section} tabIndex={-1} className="flex flex-col gap-4">
      {state.step === "choose" ? (
        <Choose
          chosen={state.id}
          onPick={(id) => dispatch({ type: "pick", id })}
          onCheck={(backup) => dispatch({ type: "check", backup })}
          onCancel={onCancel}
        />
      ) : null}
      {state.step === "check" && state.id !== null ? (
        <Check
          id={state.id}
          onBack={() => dispatch({ type: "choose" })}
          onContinue={() => dispatch({ type: "owner" })}
        />
      ) : null}
      {state.step === "owner" ? (
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (!canRestore || state.id === null || ownerId === null) return;
            restore.mutate(
              { id: state.id, ownerId, pin: state.pin },
              {
                onSuccess: (done) => dispatch({ type: "done", done }),
                onError: () => dispatch({ type: "pin", pin: "" }),
              },
            );
          }}
        >
          <h2 id="recovery-owner" className="font-semibold">
            {t("common:recovery.owner")}
          </h2>
          <RadioRows
            labelledBy="recovery-owner"
            items={owners}
            checked={ownerId}
            onCheck={(id) => dispatch({ type: "ownerId", ownerId: id })}
          >
            {(owner) => (
              <>
                <Avatar name={owner.displayName} />
                <span className="font-semibold">{owner.displayName}</span>
              </>
            )}
          </RadioRows>
          <Field id="recovery-pin" label={t("common:recovery.pin")}>
            <PinInput
              id="recovery-pin"
              value={state.pin}
              onChange={(pin) => dispatch({ type: "pin", pin })}
            />
          </Field>
          <p className="text-ink-2">{t("common:recovery.moveAside")}</p>
          {restoreError ? (
            <p role="alert" className="text-danger">
              {restoreError}
            </p>
          ) : null}
          <Footer>
            <Button
              variant="outline"
              onClick={() => {
                restore.reset(); // the refusal was about this backup or this PIN
                dispatch({ type: "choose" });
              }}
              disabled={restore.isPending}
            >
              {t("common:recovery.other")}
            </Button>
            <Button type="submit" variant="primary" disabled={!canRestore}>
              {t("common:recovery.restore")}
            </Button>
          </Footer>
          {allowFresh ? (
            <div className="flex flex-col gap-2 border-t border-line pt-4">
              <p className="text-ink-2">{t("common:recovery.fresh.explain")}</p>
              {confirmFresh ? (
                <Footer>
                  <Button variant="outline" onClick={() => setConfirmFresh(false)} disabled={fresh.isPending}>
                    {t("common:actions.cancel")}
                  </Button>
                  <Button
                    variant="danger"
                    disabled={!canRestore}
                    onClick={() => {
                      if (state.id === null || ownerId === null) return;
                      fresh.mutate(
                        { id: state.id, ownerId, pin: state.pin },
                        {
                          onSuccess: () => setFreshDone(true),
                          onError: () => dispatch({ type: "pin", pin: "" }),
                        },
                      );
                    }}
                  >
                    {t("common:recovery.fresh.confirm")}
                  </Button>
                </Footer>
              ) : (
                <div>
                  <Button variant="outline" disabled={!canRestore} onClick={() => setConfirmFresh(true)}>
                    {t("common:recovery.fresh.start")}
                  </Button>
                </div>
              )}
            </div>
          ) : null}
        </form>
      ) : null}
      {freshDone ? (
        <p role="status" className="font-semibold">
          {t("common:recovery.fresh.restarting")}
        </p>
      ) : null}
      {state.step === "done" && state.done ? (
        <>
          <p role="status" className="font-semibold">
            {t("common:recovery.restarting", { date: ltr(formatDate(state.at, lang, { withTime: true })) })}
          </p>
          <DamagedFolder path={state.done.damagedFolder} />
        </>
      ) : null}
    </section>
  );
}
