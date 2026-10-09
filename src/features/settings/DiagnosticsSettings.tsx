import { FileArchive, FileText } from "lucide-react";
import { Fragment, type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { ltrIn } from "@/features/updates/ready";
import { formatNumber } from "@/lib/format";
import { useOpenFolder } from "@/lib/ipc/app";
import { useDiagnostics, useExportDiagnostic, useSetLogLevel } from "@/lib/ipc/diagnostics";
import type { CrashRecord, DiagnosticInfo, JsonValue, LogLevel, Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { DemoCard } from "./DemoCard";
import { SupportCard } from "./SupportCard";

const LOG_LEVELS: readonly LogLevel[] = ["warn", "info", "debug"];

/** PLT-019 Réglages › Diagnostic: this PC as support needs to know it, the zip for support (saved
 * where the owner chooses, sent by hand), the technical log detail, the support contact, then the
 * demo shop for owners. */
export function DiagnosticsSettings({ session }: { session: Session }) {
  const info = useDiagnostics();
  const errorText = useErrorText();
  if (info.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(info.error)}
      </p>
    );
  }
  if (!info.data) return <Splash inline />;
  return (
    <div className="flex flex-col gap-6">
      <ThisPcCard info={info.data} />
      <SendCard logLevel={info.data.logLevel} />
      <SupportCard session={session} />
      {session.isOwner ? <DemoCard /> : null}
    </div>
  );
}

/** `system_info`'s object (its keys are documented in `src-tauri/src/diagnostics.rs`). */
function systemOf(value: JsonValue): { [key: string]: JsonValue } {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value : {};
}

const numberOf = (value: JsonValue | undefined) => (typeof value === "number" ? value : null);
const textOf = (value: JsonValue | undefined) => (typeof value === "string" && value !== "" ? value : null);

const MEGABYTES_PER_GIGABYTE = 1_024;

function ThisPcCard({ info }: { info: DiagnosticInfo }) {
  const { t } = useTranslation();
  const lang = useLang();
  const system = systemOf(info.system);
  const megabytes = (key: string) => {
    const value = numberOf(system[key]);
    return value === null ? null : t("common:units.sizeMbWhole", { value: formatNumber(value, lang) });
  };
  const total = megabytes("ramTotalMb");
  const available = megabytes("ramAvailableMb");
  const coldStart = numberOf(system.coldStartMs);
  const diskFree = numberOf(system.dataDiskFreeMb);
  const crashDumps = numberOf(system.crashDumps);
  const rows: [string, string | null][] = [
    ["settings:diagnostics.version", textOf(system.appVersion)],
    ["settings:diagnostics.webview", textOf(system.webview2)],
    ["settings:diagnostics.windows", textOf(system.os)],
    ["settings:diagnostics.memory", total !== null && available !== null ? `${total} / ${available}` : null],
    ["settings:diagnostics.appMemory", megabytes("processMemoryMb")],
    [
      "settings:diagnostics.coldStart",
      coldStart === null
        ? null
        : t("settings:diagnostics.coldStartValue", { seconds: formatNumber(coldStart / 1_000, lang, 1) }),
    ],
    [
      "settings:diagnostics.diskFree",
      diskFree === null
        ? null
        : t("common:units.sizeGb", { value: formatNumber(diskFree / MEGABYTES_PER_GIGABYTE, lang, 1) }),
    ],
    ["settings:diagnostics.crashDumps", crashDumps === null ? null : formatNumber(crashDumps, lang)],
  ];
  return (
    <Card aria-labelledby="diagnostics-pc-title">
      <h2 id="diagnostics-pc-title" className="text-xl font-bold">
        {t("settings:diagnostics.thisPc")}
      </h2>
      <dl className="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-2">
        {rows.map(([key, value]) => (
          <Fragment key={key}>
            <dt className="font-semibold">{t(key)}</dt>
            <dd>{value === null ? <span aria-hidden>—</span> : <bdi dir="ltr">{value}</bdi>}</dd>
          </Fragment>
        ))}
      </dl>
      <LastCrash crash={info.lastCrash} />
    </Card>
  );
}

function LastCrash({ crash }: { crash: CrashRecord | null }): ReactNode {
  const { t } = useTranslation();
  const lang = useLang();
  if (!crash) return <p className="text-ink-2">{t("settings:diagnostics.noCrash")}</p>;
  return (
    <p className="break-words text-warn">
      {t("settings:diagnostics.lastCrash", {
        at: ltrIn(crash.at, lang),
        message: ltrIn(crash.message, lang),
      })}
    </p>
  );
}

function SendCard({ logLevel }: { logLevel: LogLevel }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const exportDiagnostic = useExportDiagnostic();
  const openFolder = useOpenFolder();
  const setLevel = useSetLogLevel();
  const [saved, setSaved] = useState<number | null>(null);
  // The choice shows at once; the screen reads the saved level again once Rust applied it.
  const level = setLevel.isPending && setLevel.variables ? setLevel.variables : logLevel;
  const error = exportDiagnostic.error ?? openFolder.error ?? setLevel.error;
  return (
    <Card aria-labelledby="diagnostics-send-title">
      <h2 id="diagnostics-send-title" className="text-xl font-bold">
        {t("settings:diagnostics.sendTitle")}
      </h2>
      <p className="text-ink-2">{t("settings:diagnostics.sendBody")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="primary"
          disabled={exportDiagnostic.isPending}
          onClick={() => {
            setSaved(null);
            // `null`: the save dialog was cancelled.
            exportDiagnostic.mutate(undefined, {
              onSuccess: (summary) => setSaved(summary ? summary.entries.length : null),
            });
          }}
        >
          <FileArchive aria-hidden />
          {t("settings:diagnostics.export")}
        </Button>
        <Button variant="outline" onClick={() => openFolder.mutate("logs")}>
          <FileText aria-hidden />
          {t("settings:diagnostics.openLogs")}
        </Button>
      </div>
      {saved !== null ? (
        <p role="status" className="font-semibold text-ok">
          {t("settings:diagnostics.exported", { count: saved })}
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="text-danger">
          {errorText(error)}
        </p>
      ) : null}
      <Field
        id="diagnostics-log-level"
        label={t("settings:diagnostics.logLevel")}
        hint={t("settings:diagnostics.logLevelHint")}
      >
        <NativeSelect
          id="diagnostics-log-level"
          className="max-w-xs"
          value={level}
          disabled={setLevel.isPending}
          onChange={(event) => setLevel.mutate(event.target.value as LogLevel)}
        >
          {LOG_LEVELS.map((value) => (
            <option key={value} value={value}>
              {t(`settings:diagnostics.levels.${value}`)}
            </option>
          ))}
        </NativeSelect>
      </Field>
    </Card>
  );
}
