import type { DocumentPrintModel } from "@/lib/ipc/types";
import { Frame, LegalBlock, LinesTable, Pair, PartyCard, Stamp, TotalsBlock } from "./parts";

/** Bon de commande (BC), bon de réception (RC) and facture fournisseur (FF): the shop is the
 * supplier's customer, the counterpart is the supplier. */
export function PurchaseDocument({ m }: { m: DocumentPrintModel }) {
  return (
    <Frame m={m}>
      <div className="doc-parties">
        <PartyCard m={m} p={m.seller} title={<Pair m={m} k="customer" />} />
        {m.counterpart ? <PartyCard m={m} p={m.counterpart} title={<Pair m={m} k="supplier" />} /> : null}
      </div>
      <LinesTable m={m} />
      {m.showPrices ? <TotalsBlock m={m} /> : null}
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
