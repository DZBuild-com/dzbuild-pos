import { ArrowLeft, HandCoins, PackageCheck } from "lucide-react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { useApproval } from "@/components/manager-approval";
import { Splash } from "@/components/splash";
import { StatTile } from "@/components/stat-tile";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { DepositDialog, RefundDialog } from "@/features/services/DepositDialog";
import { isApprovalCancelled } from "@/lib/approval";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import {
  useCloseOrder,
  useDeliverOrder,
  useKeepDeposit,
  useOrder,
  usePayIn,
  useSetOrderReady,
} from "@/lib/ipc/orders";
import { useDeposits } from "@/lib/ipc/services";
import type { Order, Permission } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** What the till receives to serve an order (`SaleScreen` reads it from the router). */
export interface FulfilState {
  fulfil: string;
}

type Confirm = "deliver" | "invoice" | "keep" | "close";
type Open = "payIn" | "refund" | Confirm | null;

const STATUS_TONES = { open: "info", ready: "ok", fulfilled: "neutral", closed: "neutral" } as const;

function BackLink() {
  const { t } = useTranslation();
  return (
    <Link
      to="/customer-orders"
      className="inline-flex min-h-11 items-center gap-2 self-start font-semibold text-ink-2 hover:text-ink"
    >
      <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
      {t("orders:title")}
    </Link>
  );
}

/** A yes/no before an action that cannot be undone, with an optional note. */
function ConfirmDialog({
  title,
  body,
  action,
  withNote = false,
  pending,
  error,
  onConfirm,
  onClose,
}: {
  title: string;
  body: string;
  action: string;
  withNote?: boolean;
  pending: boolean;
  error: unknown;
  onConfirm: (note: string | null) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const [note, setNote] = useState("");
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={title}
      description={body}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={pending}
            onClick={() => onConfirm(note.trim() === "" ? null : note.trim())}
          >
            {action}
          </Button>
        </>
      }
    >
      {withNote ? (
        <Field id="order-confirm-note" label={t("orders:dialog.note")} optional={t("orders:optional")}>
          <Input
            id="order-confirm-note"
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
      ) : null}
      {error ? (
        <p role="alert" className="text-danger">
          {errorText(error)}
        </p>
      ) : null}
    </Dialog>
  );
}

/**
 * A customer order or layaway (ORD-002/003): its lines, the stock it reserves, the deposits held
 * and its history; by status — "Encaisser un acompte", "Prête", "Servir en caisse" (the till loads
 * it with the deposit as a voucher `CC…`), "Livrer (BL)" or "Facturer" (M3's validated document),
 * "Conserver les arrhes" (a manager's approval) and "Clôturer", refused while a deposit is held,
 * which names the two ways out (refund or keep the arrhes). Every figure is the backend's.
 */
export function OrderPage({ id }: { id: string }) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const session = useSession();
  const order = useOrder(id);
  const deposits = useDeposits("customer_order", id);
  const payIn = usePayIn();
  const ready = useSetOrderReady();
  const deliver = useDeliverOrder();
  const keep = useKeepDeposit();
  const close = useCloseOrder();
  const approval = useApproval();
  const [open, setOpen] = useState<Open>(null);
  const [problem, setProblem] = useState<unknown>(null);
  const can = (p: Permission) => session.data?.permissions.includes(p) ?? false;

  if (order.error) {
    return (
      <div className="flex flex-col gap-4">
        <BackLink />
        <p role="alert" className="text-danger">
          {errorText(order.error)}
        </p>
      </div>
    );
  }
  if (!order.data) return <Splash inline />;
  const data: Order = order.data;
  const live = data.status === "open" || data.status === "ready";
  const held = data.held > 0;
  const heading = t("orders:page.for", { number: data.number, customer: data.customerName });
  const dismiss = () => {
    setOpen(null);
    setProblem(null);
  };
  const run = async (action: () => Promise<unknown>, done: (result: unknown) => void) => {
    setProblem(null);
    try {
      done(await approval.run(action));
    } catch (error) {
      if (!isApprovalCancelled(error)) setProblem(error);
    }
  };
  const serveAs = (target: "delivery_note" | "invoice") =>
    run(
      () =>
        deliver.mutateAsync({
          id: data.id,
          target,
          // an invoice goes on the customer's account; the deposit held then settles part of it
          ...(target === "invoice"
            ? {
                payments: [
                  {
                    method: "credit",
                    amount: data.total,
                    reference: null,
                    chequeBank: null,
                    chequeDate: null,
                  },
                ],
              }
            : {}),
        }),
      (result) => {
        const done = result as { document: { id: string; number: string | null } };
        toast.success(t("orders:page.served", { number: done.document.number ?? "" }));
        dismiss();
        void navigate(`/documents/${done.document.id}`);
      },
    );

  const confirms: Record<Confirm, { title: string; body: string; action: string; note?: boolean }> = {
    deliver: {
      title: t("orders:page.deliver"),
      body: t("orders:page.deliverBody"),
      action: t("orders:page.deliver"),
    },
    invoice: {
      title: t("orders:page.invoice"),
      body: t("orders:page.invoiceBody"),
      action: t("orders:page.invoice"),
    },
    keep: {
      title: t("orders:page.keep"),
      body: t("orders:page.keepBody"),
      action: t("orders:page.keep"),
      note: true,
    },
    close: {
      title: t("orders:page.close"),
      body: t("orders:page.closeBody"),
      action: t("orders:page.close"),
      note: true,
    },
  };
  const confirm = (kind: Confirm, note: string | null) => {
    if (kind === "deliver") void serveAs("delivery_note");
    else if (kind === "invoice") void serveAs("invoice");
    else if (kind === "keep") {
      void run(
        () => keep.mutateAsync({ id: data.id, note }),
        () => {
          toast.success(t("orders:page.kept"));
          dismiss();
        },
      );
    } else {
      void run(
        () => close.mutateAsync({ id: data.id, note }),
        () => {
          toast.success(t("orders:page.closed"));
          dismiss();
        },
      );
    }
  };

  const actions: ReactNode[] = [];
  if (live && can("sales.create")) {
    actions.push(
      <Button
        key="fulfil"
        variant="primary"
        onClick={() => void navigate("/pos", { state: { fulfil: data.id } })}
      >
        {t("orders:page.fulfil")}
      </Button>,
      <Button key="payIn" variant="secondary" onClick={() => setOpen("payIn")}>
        <HandCoins aria-hidden />
        {t("orders:page.payIn")}
      </Button>,
    );
    if (data.status === "open") {
      actions.push(
        <Button
          key="ready"
          variant="secondary"
          disabled={ready.isPending}
          onClick={() =>
            ready.mutate(
              { id: data.id },
              {
                onSuccess: () => toast.success(t("orders:page.readyDone")),
                onError: (error) => toast.error(errorText(error)),
              },
            )
          }
        >
          <PackageCheck aria-hidden />
          {t("orders:page.ready")}
        </Button>,
      );
    }
  }
  if (live && can("documents.delivery")) {
    actions.push(
      <Button key="deliver" variant="secondary" onClick={() => setOpen("deliver")}>
        {t("orders:page.deliver")}
      </Button>,
    );
  }
  if (live && can("documents.invoice")) {
    actions.push(
      <Button key="invoice" variant="secondary" onClick={() => setOpen("invoice")}>
        {t("orders:page.invoice")}
      </Button>,
    );
  }
  if (live && held && can("sales.create")) {
    actions.push(
      <Button key="keep" variant="outline" onClick={() => setOpen("keep")}>
        {t("orders:page.keep")}
      </Button>,
    );
  }
  if (live && held && can("sales.return")) {
    actions.push(
      <Button key="refund" variant="outline" onClick={() => setOpen("refund")}>
        {t("services:deposit.refund")}
      </Button>,
    );
  }
  if (live && can("documents.quote")) {
    actions.push(
      <Button key="close" variant="danger" disabled={held} onClick={() => setOpen("close")}>
        {t("orders:page.close")}
      </Button>,
    );
  }

  return (
    <div className="flex flex-col gap-6">
      <BackLink />
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-3xl font-bold">
          <bdi dir="ltr">{data.number}</bdi> · {data.customerName}
        </h1>
        <Badge tone={STATUS_TONES[data.status]}>{t(`orders:status.${data.status}`)}</Badge>
        <Badge tone={data.kind === "layaway" ? "warn" : "neutral"}>{t(`orders:kinds.${data.kind}`)}</Badge>
      </div>

      <div className="grid gap-3 sm:grid-cols-3">
        <StatTile label={t("orders:list.total")} value={<Amount value={data.total} />} />
        <StatTile label={t("orders:list.held")} value={<Amount value={data.held} />} />
        <StatTile
          label={t("orders:dialog.due")}
          value={data.dueOn ? <bdi dir="ltr">{formatDate(dayToMs(data.dueOn), lang)}</bdi> : "—"}
        />
      </div>
      {data.fulfilledDocumentId && data.fulfilledNumber ? (
        <Link to={`/documents/${data.fulfilledDocumentId}`} className="self-start font-semibold underline">
          {t("orders:page.servedBy", { number: data.fulfilledNumber })}
        </Link>
      ) : null}

      {actions.length > 0 ? <div className="flex flex-wrap gap-2">{actions}</div> : null}
      {live && held ? <p className="text-ink-2">{t("orders:page.heldHint")}</p> : null}
      {live && data.kind === "layaway" ? <p className="text-ink-2">{t("orders:page.layawayHint")}</p> : null}
      {problem && open === null ? (
        <p role="alert" className="text-danger">
          {errorText(problem)}
        </p>
      ) : null}

      <section className="flex flex-col gap-3">
        <h2 className="text-xl font-bold">{t("orders:page.lines")}</h2>
        <table aria-label={t("orders:page.lines")} className="w-full max-w-4xl border-collapse">
          <thead>
            <tr className="border-b border-line text-ink-2">
              <th scope="col" className="py-2 text-start font-semibold">
                {t("orders:page.item")}
              </th>
              <th scope="col" className="py-2 text-end font-semibold">
                {t("orders:page.qty")}
              </th>
              <th scope="col" className="py-2 text-end font-semibold">
                {t("orders:page.unitPrice")}
              </th>
              <th scope="col" className="py-2 text-end font-semibold">
                {t("orders:list.total")}
              </th>
            </tr>
          </thead>
          <tbody>
            {data.lines.map((line, index) => (
              <tr
                key={`${line.productId ?? "open"}-${index}`}
                className="border-b border-line even:bg-surface-2"
              >
                <td className="py-2">{line.description}</td>
                <td className="py-2 text-end tabular-nums">
                  <bdi dir="ltr">{formatQty(line.qty, lang)}</bdi>
                </td>
                <td className="py-2 text-end">
                  <Amount value={line.unitPrice} />
                </td>
                <td className="py-2 text-end">
                  <Amount value={line.totalTtc} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      {data.reserved.length > 0 ? (
        <section className="flex flex-col gap-2">
          <h2 className="text-xl font-bold">{t("orders:page.reserved")}</h2>
          <ul className="flex flex-col gap-1">
            {data.reserved.map((r) => (
              <li key={r.productId} className="flex gap-3">
                <span>{r.name}</span>
                <bdi dir="ltr" className="text-ink-2 tabular-nums">
                  {formatQty(r.qty, lang)}
                </bdi>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {deposits.data && deposits.data.length > 0 ? (
        <section className="flex flex-col gap-2">
          <h2 className="text-xl font-bold">{t("orders:page.deposits")}</h2>
          <ul className="flex flex-col gap-1">
            {deposits.data.map((d) => (
              <li key={d.id} className="flex flex-wrap items-center gap-x-4">
                <bdi dir="ltr" className="text-ink-2">
                  {formatDate(d.createdAt, lang, { withTime: true })}
                </bdi>
                <span>{t(`orders:depositKinds.${d.kind}`)}</span>
                <Amount value={d.amount} className="font-semibold" />
                {d.method ? <span>{t(`payments:tenders.${d.method}`)}</span> : null}
                {d.receiptNumber ? <bdi dir="ltr">{d.receiptNumber}</bdi> : null}
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      <section className="flex flex-col gap-2">
        <h2 className="text-xl font-bold">{t("orders:page.history")}</h2>
        <ol className="flex flex-col gap-1">
          {data.events.map((e, index) => (
            <li key={`${e.createdAt}-${index}`} className="flex flex-wrap gap-x-3">
              <span className="font-semibold">{t(`orders:status.${e.status}`)}</span>
              <bdi dir="ltr" className="text-ink-2">
                {formatDate(e.createdAt, lang, { withTime: true })}
              </bdi>
              {e.userName ? (
                <span className="text-ink-2">{t("orders:page.by", { name: e.userName })}</span>
              ) : null}
              {e.note ? <span>{e.note}</span> : null}
            </li>
          ))}
        </ol>
      </section>

      {open === "payIn" ? (
        <DepositDialog
          ownerType="customer_order"
          ownerId={data.id}
          heading={heading}
          onTake={(input) =>
            payIn.mutateAsync({
              id: data.id,
              deposit: {
                amount: input.amount,
                method: input.method,
                reference: input.reference,
                chequeBank: input.chequeBank,
                chequeDate: input.chequeDate,
              },
            })
          }
          onClose={dismiss}
        />
      ) : null}
      {open === "refund" ? (
        <RefundDialog
          ownerType="customer_order"
          ownerId={data.id}
          held={data.held}
          heading={heading}
          onClose={dismiss}
        />
      ) : null}
      {open === "deliver" || open === "invoice" || open === "keep" || open === "close" ? (
        <ConfirmDialog
          title={confirms[open].title}
          body={confirms[open].body}
          action={confirms[open].action}
          withNote={confirms[open].note}
          pending={deliver.isPending || keep.isPending || close.isPending}
          error={problem}
          onConfirm={(note) => confirm(open, note)}
          onClose={dismiss}
        />
      ) : null}
      {approval.dialog}
    </div>
  );
}
