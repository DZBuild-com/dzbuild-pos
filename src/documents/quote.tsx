import type { DocumentPrintModel } from "@/lib/ipc/types";
import { Frame, LegalBlock, LinesTable, Pair, PartyCard, Stamp, TotalsBlock } from "./parts";

/** Devis (DV) and facture proforma (PF): the offer's validity and, for a PF, "ne constitue pas une
 * facture" come in the model's legal lines; no payment, no stock. */
export function QuoteDocument({ m }: { m: DocumentPrintModel }) {
  return (
    <Frame m={m}>
      <div className="doc-parties">
        <PartyCard m={m} p={m.seller} title={<Pair m={m} k="seller" />} />
        {m.counterpart ? (
          <PartyCard m={m} p={m.counterpart} title={<Pair m={m} k={m.counterpartRole} />} />
        ) : null}
      </div>
      <LinesTable m={m} />
      <TotalsBlock m={m} />
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
