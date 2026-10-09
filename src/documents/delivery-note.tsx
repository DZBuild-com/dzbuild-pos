import type { DocumentPrintModel, PrintTransfer } from "@/lib/ipc/types";
import { Frame, LegalBlock, LinesTable, Ltr, Pair, PartyCard, Stamp, TotalsBlock } from "./parts";

/** A bon de transfert's origin and destination, driver and vehicle (décret 05-468 art. 12-13). */
function TransferCard({ m, t }: { m: DocumentPrintModel; t: PrintTransfer }) {
  return (
    <section className="doc-party">
      <p>
        <Pair m={m} k="from" inline /> :{" "}
        <strong>
          <bdi>{t.from}</bdi>
        </strong>
      </p>
      <p>
        <Pair m={m} k="to" inline /> :{" "}
        <strong>
          <bdi>{t.to}</bdi>
        </strong>
      </p>
      {t.driver ? (
        <p>
          <Pair m={m} k="driver" inline /> : <bdi>{t.driver}</bdi>
        </p>
      ) : null}
      {t.vehicle ? (
        <p>
          <Pair m={m} k="vehicle" inline /> : <Ltr>{t.vehicle}</Ltr>
        </p>
      ) : null}
    </section>
  );
}

/** Bon de livraison (BL) and bon de transfert (BT): prices only when the model shows them, the
 * receiver's box beside the stamp. */
export function DeliveryDocument({ m }: { m: DocumentPrintModel }) {
  return (
    <Frame m={m}>
      <div className="doc-parties">
        {/* a transfer moves goods between the shop's own places: the issuer, not a seller */}
        <PartyCard m={m} p={m.seller} title={m.transfer ? undefined : <Pair m={m} k="seller" />} />
        {m.transfer ? <TransferCard m={m} t={m.transfer} /> : null}
        {!m.transfer && m.counterpart ? (
          <PartyCard m={m} p={m.counterpart} title={<Pair m={m} k={m.counterpartRole} />} />
        ) : null}
      </div>
      <LinesTable m={m} />
      {m.showPrices ? <TotalsBlock m={m} /> : null}
      <LegalBlock m={m} />
      {m.note ? (
        <p>
          <Pair m={m} k="note" inline /> : {m.note}
        </p>
      ) : null}
      <div className="doc-signs">
        <div className="doc-received">
          <Pair m={m} k="receivedBy" />
        </div>
        <Stamp m={m} />
      </div>
    </Frame>
  );
}
