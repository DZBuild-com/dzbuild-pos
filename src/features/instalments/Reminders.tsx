import { BellRing, Copy } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { EmptyState } from "@/components/empty-state";
import { Button } from "@/components/ui/button";
import { NumberSetting, TrilingualSetting } from "@/features/packs/setting-cards";
import { useCan } from "@/features/stock/stock-parts";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useCustomer } from "@/lib/ipc/customers";
import { useNoteReminder, usePlan, useReminders, useReminderText } from "@/lib/ipc/instalments";
import type { Reminder } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { ConsentLines } from "./ConsentLines";
import { DueBadge } from "./PlanPage";

/**
 * TMP-007 ("Relances"): the dues coming within `instalments.reminder_days` and the late ones. Each
 * opens its reminder in the customer's language to copy into WhatsApp or an SMS (the POS sends
 * nothing), beside the customer's consent on each channel, and "Noter le rappel" journals it.
 */
export function Reminders() {
  const can = useCan();
  return (
    <div className="flex flex-col gap-6">
      <ReminderList />
      {can("settings.company") ? <ReminderSettings /> : null}
    </div>
  );
}

/** TMP-007 (final review): how many days ahead a due is listed, and the reminder's text. */
function ReminderSettings() {
  const { t } = useTranslation();
  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <NumberSetting
        settingKey="instalments.reminder_days"
        id="instalments-reminder-days"
        title={t("packs:settings.reminders.days")}
        label={t("packs:settings.reminders.daysLabel")}
        help={t("packs:settings.reminders.daysHelp")}
        min={0}
        max={30}
      />
      <TrilingualSetting
        settingKey="instalments.reminder_message"
        id="instalments-reminder-message"
        title={t("packs:settings.reminders.message")}
        help={t("packs:settings.reminders.messageHelp")}
        max={300}
      />
    </div>
  );
}

function ReminderList() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const reminders = useReminders();
  const [openKey, setOpenKey] = useState<string | null>(null);
  if (reminders.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(reminders.error)}
      </p>
    );
  }
  if (!reminders.data) return <p className="text-ink-2">{t("common:status.loading")}</p>;
  if (reminders.data.length === 0) {
    return (
      <EmptyState
        icon={BellRing}
        title={t("instalments:reminders.emptyTitle")}
        body={t("instalments:reminders.emptyBody")}
      />
    );
  }
  return (
    <ul aria-label={t("instalments:reminders.title")} className="flex flex-col gap-3">
      {reminders.data.map((reminder) => {
        const key = `${reminder.planId}:${reminder.due.seq}`;
        return (
          <ReminderCard
            key={key}
            reminder={reminder}
            open={openKey === key}
            onToggle={() => setOpenKey((current) => (current === key ? null : key))}
          />
        );
      })}
    </ul>
  );
}

function ReminderCard({
  reminder,
  open,
  onToggle,
}: {
  reminder: Reminder;
  open: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  return (
    <li className="flex flex-col gap-3 rounded-card border border-line bg-surface p-4">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <Link to={`/instalments/${reminder.planId}`} className="font-bold underline">
          <bdi dir="ltr">{reminder.number}</bdi>
        </Link>
        <span className="font-semibold">{reminder.customerName}</span>
        <bdi dir="ltr" className="text-ink-2">
          {formatDate(dayToMs(reminder.due.dueOn), lang)}
        </bdi>
        <Amount value={reminder.due.amount} className="font-semibold" />
        <DueBadge due={reminder.due} />
        <Button variant="secondary" className="ms-auto" aria-expanded={open} onClick={onToggle}>
          {t("instalments:reminders.prepare")}
        </Button>
      </div>
      {open ? <ReminderDetail reminder={reminder} /> : null}
    </li>
  );
}

/** The text in the customer's language (his file's, else the UI's), his consent, copy and note. */
function ReminderDetail({ reminder }: { reminder: Reminder }) {
  const { t } = useTranslation();
  const uiLang = useLang();
  const errorText = useErrorText();
  const plan = usePlan(reminder.planId);
  const customerId = plan.data?.customerId ?? null;
  const customer = useCustomer(customerId);
  const lang = customer.data?.lang ?? uiLang;
  // asked once the customer's language is known (or his file cannot be read)
  const ready = plan.isSuccess && (customer.isSuccess || customer.isError);
  const message = useReminderText(ready ? reminder.planId : null, reminder.due.seq, lang);
  const note = useNoteReminder();
  const [copied, setCopied] = useState(false);
  const text = message.data;
  return (
    <div className="flex flex-col gap-3">
      {text ? (
        <p className="rounded-card bg-surface-2 p-4" lang={lang} dir={lang === "ar" ? "rtl" : "ltr"}>
          {text}
        </p>
      ) : message.error ? (
        <p role="alert" className="text-danger">
          {errorText(message.error)}
        </p>
      ) : (
        <p className="text-ink-2">{t("common:status.loading")}</p>
      )}
      <ConsentLines customerId={customerId} />
      <div className="flex flex-wrap items-center gap-3">
        <Button
          variant="secondary"
          disabled={!text}
          onClick={() => {
            if (text) void navigator.clipboard.writeText(text).then(() => setCopied(true));
          }}
        >
          <Copy aria-hidden />
          {t("instalments:reminders.copy")}
        </Button>
        <Button
          variant="outline"
          disabled={note.isPending || note.isSuccess}
          onClick={() =>
            note.mutate(
              { planId: reminder.planId, seq: reminder.due.seq },
              {
                onSuccess: () => toast.success(t("instalments:reminders.noted")),
                onError: (error) => toast.error(errorText(error)),
              },
            )
          }
        >
          {t("instalments:reminders.note")}
        </Button>
        {copied ? <span role="status">{t("instalments:reminders.copied")}</span> : null}
      </div>
      <p className="text-sm text-ink-2">{t("instalments:reminders.hint")}</p>
    </div>
  );
}
