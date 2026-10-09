import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import type { ScanKey } from "@/lib/scanner";
import { type Learned, learnScanner, TEST_PAGE_CODE } from "@/lib/scanner-learn";

/** A scanner that sends no Enter or Tab has finished its code after this long without a key. */
const LEARN_SILENCE_MS = 300;
const ENDS = new Set(["Enter", "Tab", "NumpadEnter"]);
const MODIFIERS = new Set(["Shift", "CapsLock", "Control", "Alt", "Meta"]);

/** "Apprendre mon lecteur": one scan of a known code tells the scanner's keyboard layout, prefix
 * and end of scan. The capture field records the raw keys and types nothing. */
export function LearnScannerDialog({
  onClose,
  onApply,
}: {
  onClose: () => void;
  onApply: (learned: Learned) => void;
}) {
  const { t } = useTranslation();
  const [expected, setExpected] = useState(TEST_PAGE_CODE);
  // undefined: nothing scanned yet; null: the scan did not contain the expected code
  const [result, setResult] = useState<Learned | null | undefined>(undefined);
  const keys = useRef<ScanKey[]>([]);
  const silence = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(silence.current), []);
  const finish = () => {
    window.clearTimeout(silence.current);
    setResult(learnScanner(keys.current, expected.trim()));
    keys.current = [];
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (MODIFIERS.has(event.key)) return;
    event.preventDefault();
    window.clearTimeout(silence.current);
    keys.current.push({
      code: event.code,
      key: event.key,
      shiftKey: event.shiftKey,
      timeStamp: event.timeStamp,
    });
    if (ENDS.has(event.key) || ENDS.has(event.code)) finish();
    else silence.current = window.setTimeout(finish, LEARN_SILENCE_MS);
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t("hardware:scanners.learn.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={!result} onClick={() => result && onApply(result)}>
            {t("hardware:scanners.learn.apply")}
          </Button>
        </>
      }
    >
      <p className="text-ink-2">{t("hardware:scanners.learn.hint", { code: TEST_PAGE_CODE })}</p>
      <Field id="learn-expected" label={t("hardware:scanners.learn.expected")}>
        <Input
          id="learn-expected"
          dir="ltr"
          autoComplete="off"
          value={expected}
          onChange={(e) => {
            setExpected(e.target.value);
            setResult(undefined);
          }}
        />
      </Field>
      <Field id="learn-capture" label={t("hardware:scanners.learn.capture")}>
        <Input id="learn-capture" dir="ltr" autoComplete="off" value="" readOnly onKeyDown={onKeyDown} />
      </Field>
      <div aria-live="polite">
        {result ? (
          <p className="font-semibold text-ok">
            {t("hardware:scanners.learn.found", {
              layout: t(`hardware:scanner.layouts.${result.layout}`),
              prefix: result.prefix,
              suffix: t(`hardware:scanners.suffixes.${result.suffix}`),
            })}
          </p>
        ) : null}
        {result === null ? <p className="text-danger">{t("hardware:scanners.learn.failed")}</p> : null}
      </div>
    </Dialog>
  );
}
