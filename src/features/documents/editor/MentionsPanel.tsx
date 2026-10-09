import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import type { DocType, Mention, MentionCheck } from "@/lib/ipc/types";

/** Nothing blocking is missing: the TS twin of `MentionCheck::complete`. */
export function isComplete(check: MentionCheck): boolean {
  return check.sellerMissing.length === 0 && check.buyerMissing.length === 0;
}

/** Whether the kind cannot be validated while a blocking mention is missing (DOC-003). */
export function blocksOnMentions(docType: DocType): boolean {
  return docType === "invoice" || docType === "credit_note";
}

/** The seller's mentions entered in Réglages › Documents (the identity card), not on the shop's
 * screen: the nature of the activity, and the auto-entrepreneur card that stands for the RC. */
const DOCUMENT_MENTIONS: readonly Mention[] = ["activity"];

/** Where each missing seller mention is completed: the shop's screen, the documents' identity
 * card (an RC that is missing may also be replaced by the AE card there), or both. */
export function sellerScreens(check: MentionCheck): { shop: boolean; documents: boolean } {
  const seller = [...check.sellerMissing, ...check.sellerWarnings];
  return {
    shop: seller.some((m) => !DOCUMENT_MENTIONS.includes(m)),
    documents: seller.some((m) => DOCUMENT_MENTIONS.includes(m)) || check.sellerMissing.includes("rc"),
  };
}

const link =
  "inline-flex min-h-11 items-center self-start rounded-control font-semibold underline focus-visible:outline-3 focus-visible:outline-focus";

/**
 * DOC-003: what the shop's and the customer's details still lack for this document, as the
 * backend checked it (décret 05-468 art. 3), each with the screen that completes them. On an
 * invoice (or a credit note) the missing ones block, in `--danger`; on the other kinds they are
 * warnings, in `--warn`, and a document without a customer (a counter quote) lacks nothing of his.
 */
export function MentionsPanel({
  check,
  docType,
  customerId,
}: {
  check: MentionCheck;
  docType: DocType;
  customerId: string | null;
}) {
  const { t } = useTranslation();
  const titleId = useId();
  const list = (mentions: readonly Mention[]) =>
    mentions.map((m) => t(`billing:mentions.names.${m}`)).join(", ");
  const warnings = [...new Set([...check.sellerWarnings, ...check.buyerWarnings])];
  const blocks = blocksOnMentions(docType);
  const buyerMissing = blocks || check.buyerKind !== "missing" ? check.buyerMissing : [];
  const complete = check.sellerMissing.length === 0 && buyerMissing.length === 0;
  const tone = blocks ? "text-danger" : "text-warn";
  const screens = sellerScreens(check);
  return (
    <section aria-labelledby={titleId} className="flex flex-col gap-2 rounded-card border border-line p-3">
      <h2 id={titleId} className="font-bold">
        {t("billing:mentions.title")}
      </h2>
      {complete && warnings.length === 0 ? <p className="text-ok">{t("billing:mentions.complete")}</p> : null}
      {check.sellerMissing.length > 0 ? (
        <p className={tone}>{t("billing:mentions.sellerMissing", { list: list(check.sellerMissing) })}</p>
      ) : null}
      {buyerMissing.length > 0 ? (
        <p className={tone}>{t("billing:mentions.buyerMissing", { list: list(buyerMissing) })}</p>
      ) : null}
      {!complete && blocks ? (
        <p role="alert" className="font-semibold text-danger">
          {t("billing:mentions.blocking")}
        </p>
      ) : null}
      {warnings.length > 0 ? (
        <p className="text-warn">{t("billing:mentions.warnings", { list: list(warnings) })}</p>
      ) : null}
      <div className="flex flex-wrap gap-4">
        {screens.shop ? (
          <Link to="/settings/shop" className={link}>
            {t("billing:mentions.fixShop")}
          </Link>
        ) : null}
        {screens.documents ? (
          <Link to="/settings/documents#documents-identity" className={link}>
            {t("billing:mentions.fixDocuments")}
          </Link>
        ) : null}
        {customerId !== null && (buyerMissing.length > 0 || check.buyerWarnings.length > 0) ? (
          <Link to={`/customers/${customerId}`} className={link}>
            {t("billing:mentions.fixCustomer")}
          </Link>
        ) : null}
      </div>
    </section>
  );
}
