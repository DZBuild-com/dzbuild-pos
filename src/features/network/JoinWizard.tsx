import { Search, Server } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { PinInput } from "@/components/pin-pad";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formatNumber } from "@/lib/format";
import { useAppInfo } from "@/lib/ipc/app";
import { toAppError } from "@/lib/ipc/client";
import { useDiscover, useJoin, usePairProgress, useProbe, useRestart } from "@/lib/ipc/lan";
import { useSetupStatus } from "@/lib/ipc/setup";
import type { HealthInfo, JoinResult } from "@/lib/ipc/types";
import { useErrorText, useFieldError } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { CodeInput } from "./CodeInput";
import { type JoinDraft, type JoinErrors, hostAddress, validateJoin } from "./join-form";

type Stage =
  | { kind: "find" }
  | { kind: "code"; address: string; health: HealthInfo }
  | { kind: "done"; result: JoinResult };

/** What the main PC answers about the owner's PIN: shown under the PIN field, not as a code error. */
const PIN_ERRORS = new Set(["invalid_pin", "too_many_attempts"]);

/** LAN-001/002: this PC becomes a till of the main PC it finds (or whose address is typed),
 * with the 6-digit code the owner shows there. On a PC already set up (Settings › Réseau) the
 * owner's PIN confirms it, as a restore does: its books are replaced at the next start. Content
 * only: the caller frames it. */
export function JoinWizard({ onBack, titled = true }: { onBack: () => void; titled?: boolean }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const app = useAppInfo();
  const askPin = useSetupStatus(true).data?.completed === true;
  const [stage, setStage] = useState<Stage>({ kind: "find" });
  const discover = useDiscover(stage.kind === "find");
  const probe = useProbe();
  const join = useJoin();
  const restart = useRestart();
  const progress = usePairProgress();
  const [manual, setManual] = useState("");
  const [draft, setDraft] = useState<JoinDraft>({ code: "", terminalName: "", terminalCode: "" });
  const [pin, setPin] = useState("");
  const [errors, setErrors] = useState<JoinErrors & { ownerPin?: string }>({});
  const errorOf = useFieldError(errors);
  const title = titled ? <h2 className="text-2xl font-bold">{t("sync:lan.join.title")}</h2> : null;

  const check = (address: string) => {
    probe.mutate(address, {
      onSuccess: (health) => {
        setDraft({
          code: "",
          terminalCode: health.suggestedCode,
          terminalName: t("sync:lan.join.nameDefault", { code: health.suggestedCode }),
        });
        setPin("");
        setErrors({});
        join.reset();
        setStage({ kind: "code", address, health });
      },
    });
  };

  const versionOf = (health: HealthInfo) =>
    app.data === undefined
      ? "unknown"
      : health.appVersion === app.data.version && health.schemaVersion === app.data.schemaVersion
        ? "same"
        : "different";

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (stage.kind !== "code" || versionOf(stage.health) !== "same") return;
    const found: JoinErrors & { ownerPin?: string } = validateJoin(draft);
    if (askPin && pin === "") found.ownerPin = "validation:required";
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    join.mutate(
      {
        address: stage.address,
        code: draft.code,
        terminalCode: draft.terminalCode,
        terminalName: draft.terminalName.trim(),
        ...(askPin ? { ownerPin: pin } : {}),
      },
      { onSuccess: (result) => setStage({ kind: "done", result }) },
    );
  };

  if (stage.kind === "done") {
    return (
      <div className="flex flex-col gap-4">
        {title}
        <p role="status" className="text-lg">
          {t("sync:lan.join.done", { code: stage.result.terminalCode, shop: stage.result.hostName })}
        </p>
        <Button variant="primary" size="lg" disabled={restart.isPending} onClick={() => restart.mutate()}>
          {t("sync:lan.join.restart")}
        </Button>
        {restart.error ? (
          <p role="alert" className="text-danger">
            {errorText(restart.error)}
          </p>
        ) : null}
      </div>
    );
  }

  if (stage.kind === "code") {
    const { health } = stage;
    const version = versionOf(health);
    const percent =
      progress && progress.total !== null && progress.total > 0
        ? Math.min(100, Math.floor((progress.received * 100) / progress.total))
        : 0;
    const joinError = join.error ? toAppError(join.error) : null;
    const pinRefused =
      askPin && joinError !== null && (PIN_ERRORS.has(joinError.code) || joinError.field === "ownerPin");
    return (
      <form noValidate onSubmit={submit} className="flex flex-col gap-4">
        {title}
        <p className="flex flex-wrap items-center gap-2 text-lg font-semibold">
          <Server aria-hidden className="size-5 text-ink-2" />
          {t("sync:lan.join.shop", { name: health.shopName })}
          <bdi dir="ltr" className="font-mono text-base font-normal text-ink-2">
            {stage.address}
          </bdi>
        </p>
        {version === "different" ? (
          <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
            {t("sync:lan.join.versionMismatch")}
          </p>
        ) : null}
        {version === "same" && !health.pairingOpen ? (
          <p className="rounded-control bg-warn-soft p-3">{t("sync:lan.join.pairingClosed")}</p>
        ) : null}
        <Field id="join-code" label={t("sync:lan.join.code")} error={errorOf("code")}>
          <CodeInput
            id="join-code"
            value={draft.code}
            aria-invalid={errors.code !== undefined}
            onChange={(code) => setDraft({ ...draft, code })}
          />
        </Field>
        <Field id="join-name" label={t("sync:lan.join.name")} error={errorOf("terminalName")}>
          <Input
            id="join-name"
            dir="auto"
            value={draft.terminalName}
            maxLength={40}
            aria-invalid={errors.terminalName !== undefined}
            onChange={(event) => setDraft({ ...draft, terminalName: event.target.value })}
          />
        </Field>
        <Field
          id="join-series"
          label={t("sync:lan.join.series")}
          hint={t("sync:lan.join.seriesHelp", {
            year: new Date(health.hostTime).getFullYear(),
            code: draft.terminalCode || health.suggestedCode,
          })}
          error={errorOf("terminalCode")}
        >
          <Input
            id="join-series"
            dir="ltr"
            value={draft.terminalCode}
            maxLength={8}
            className="font-mono uppercase"
            aria-invalid={errors.terminalCode !== undefined}
            onChange={(event) =>
              setDraft({ ...draft, terminalCode: event.target.value.toUpperCase().replace(/[^A-Z0-9]/g, "") })
            }
          />
        </Field>
        {askPin ? (
          <Field
            id="join-pin"
            label={t("sync:lan.till.pin")}
            error={errorOf("ownerPin") ?? (pinRefused && joinError ? errorText(joinError) : undefined)}
          >
            <PinInput
              id="join-pin"
              value={pin}
              aria-invalid={errors.ownerPin !== undefined || pinRefused}
              onChange={setPin}
            />
          </Field>
        ) : null}
        <p className="text-sm text-ink-2">{t("sync:lan.join.keepsData")}</p>
        {join.isPending ? (
          <div className="flex flex-col gap-2" role="status">
            <p>{t("sync:lan.join.downloading", { percent: formatNumber(percent, lang) })}</p>
            <div className="h-2 overflow-hidden rounded-full bg-surface-2">
              <div
                className="h-full bg-brand transition-[inline-size]"
                style={{ inlineSize: `${percent}%` }}
              />
            </div>
          </div>
        ) : null}
        {joinError && !pinRefused ? (
          <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
            {errorText(joinError)}
          </p>
        ) : null}
        <div className="flex justify-end gap-2">
          <Button
            variant="outline"
            disabled={join.isPending}
            onClick={() => {
              join.reset();
              setStage({ kind: "find" });
            }}
          >
            {t("sync:lan.join.back")}
          </Button>
          <Button type="submit" variant="primary" disabled={version !== "same" || join.isPending}>
            {t("sync:lan.join.submit")}
          </Button>
        </div>
      </form>
    );
  }

  const hosts = discover.data ?? [];
  return (
    <div className="flex flex-col gap-4">
      {title}
      <p>{t("sync:lan.join.intro")}</p>
      {discover.isFetching ? <p role="status">{t("sync:lan.join.searching")}</p> : null}
      {discover.error ? (
        // a refusal before any network call (paired tills, a DZBuild link), or discovery itself
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {errorText(discover.error)}
        </p>
      ) : null}
      {!discover.isFetching && !discover.error && hosts.length === 0 ? (
        <p>{t("sync:lan.join.none")}</p>
      ) : null}
      {hosts.length > 0 ? (
        <ul className="flex flex-col gap-2" aria-label={t("sync:lan.join.found")}>
          {hosts.map((host) => {
            const address = hostAddress(host);
            return (
              <li key={host.hostId} className="flex items-center gap-3 rounded-card border border-line p-3">
                <Server aria-hidden className="size-6 text-ink-2" />
                <div className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate font-semibold">{host.name}</span>
                  {address ? (
                    <bdi dir="ltr" className="font-mono text-sm text-ink-2">
                      {address}
                    </bdi>
                  ) : null}
                </div>
                <Button
                  variant="primary"
                  disabled={address === null || probe.isPending}
                  onClick={() => address && check(address)}
                >
                  {t("sync:lan.join.choose")}
                </Button>
              </li>
            );
          })}
        </ul>
      ) : null}
      <form
        noValidate
        className="flex flex-col gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (manual.trim() !== "") check(manual.trim());
        }}
      >
        <Field id="join-address" label={t("sync:lan.join.address")} hint={t("sync:lan.join.addressHelp")}>
          <div className="flex gap-2">
            <Input
              id="join-address"
              dir="ltr"
              inputMode="url"
              autoComplete="off"
              spellCheck={false}
              value={manual}
              className="font-mono"
              onChange={(event) => setManual(event.target.value)}
            />
            <Button type="submit" variant="outline" disabled={manual.trim() === "" || probe.isPending}>
              <Search aria-hidden />
              {t("sync:lan.join.check")}
            </Button>
          </div>
        </Field>
      </form>
      {probe.error ? (
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {errorText(probe.error)}
        </p>
      ) : null}
      <div className="flex justify-start">
        <Button variant="outline" onClick={onBack}>
          {t("sync:lan.join.back")}
        </Button>
      </div>
    </div>
  );
}
