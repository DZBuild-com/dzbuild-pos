import type { DocumentPrintModel } from "@/lib/ipc/types";
import { Frame, LegalBlock, LinesTable, Pair, PartyCard, PaymentBlock, Stamp, TotalsBlock } from "./parts";

/** Facture (FA) and avoir (AV): every mention of décret 05-468 art. 3, the stamp duty on its own
 * line, the amount in words, the stamp-and-signature box. */
export function InvoiceDocument({ m }: { m: DocumentPrintModel }) {
  return (
    <Frame m={m}>
      <div className="doc-parties">
        <PartyCard m={m} p={m.seller} title={<Pair m={m} k="seller" />} />
        {m.counterpart ? (
          <PartyCard m={m} p={m.counterpart} title={<Pair m={m} k={m.counterpartRole} />} />
        ) : null}
      </div>
      {m.creditKind ? (
        <p className="doc-legal">
          {m.creditKind}
          {m.reason ? (
            <>
              {" — "}
              <Pair m={m} k="reason" inline /> : {m.reason}
            </>
          ) : null}
        </p>
      ) : null}
      <LinesTable m={m} />
      <TotalsBlock m={m} />
      <PaymentBlock m={m} />
      <LegalBlock m={m} />
      {m.note ? (
        <p>
          <Pair m={m} k="note" inline /> : {m.note}
        </p>
      ) : null}
      <Stamp m={m} />
    </Frame>
  );
}
