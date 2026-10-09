import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import type { HidSuffix, ScannerLayout } from "@/lib/ipc/types";
import { ScanDetector } from "@/lib/scanner";

/** Spec §8: "test your scanner" — what the sale screen will read with the chosen layout, prefix
 * and end of scan. */
export function ScannerTest({
  layout,
  prefix = "",
  suffix = "enter_or_tab",
}: {
  layout: ScannerLayout;
  prefix?: string;
  suffix?: HidSuffix;
}) {
  const { t } = useTranslation();
  const detector = useMemo(() => new ScanDetector({ layout, prefix, suffix }), [layout, prefix, suffix]);
  const silence = useRef<number | undefined>(undefined);
  const [read, setRead] = useState<string | null>(null);
  const [value, setValue] = useState("");
  useEffect(() => () => window.clearTimeout(silence.current), [detector]);
  const show = (code: string) => {
    setRead(code);
    setValue("");
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    window.clearTimeout(silence.current);
    const code = detector.feed(event);
    if (code !== null) {
      event.preventDefault();
      show(code);
    } else {
      silence.current = detector.afterSilence(event, show);
    }
  };
  return (
    <div className="flex flex-col gap-2">
      <Field id="scanner-test" label={t("hardware:scanner.test")} hint={t("hardware:scanner.testHint")}>
        <Input
          id="scanner-test"
          dir="ltr"
          autoComplete="off"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={onKeyDown}
        />
      </Field>
      {read !== null ? (
        <p role="status" className="font-semibold text-ok">
          {t("hardware:scanner.read", { code: read })}
        </p>
      ) : null}
    </div>
  );
}
