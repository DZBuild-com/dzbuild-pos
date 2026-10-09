/** What "Encaisser" asks before the payment dialog, in this order (M8): the serials of the lines
 * still short of them (SAL-025), the register entries of the controlled medicines (pharmacy), the
 * buyer of the precious-metal pieces when the customer's file does not hold the identity
 * (jeweller register). Each is a dialog completed or cancelled. */
export type CheckoutStep =
  | { kind: "serials"; line: number; productId: string; name: string; count: number }
  | { kind: "prescriptions"; lines: { line: number; name: string }[] }
  | { kind: "buyer" };

/** One cart line as the steps read it. */
export interface CheckoutLine {
  productId: string | null;
  name: string;
  /** Whole pieces (one serial each). */
  pieces: number;
  /** Serials already given for the line. */
  serials: number;
  trackSerials: boolean;
  controlled: boolean;
  jewel: boolean;
}

export interface CheckoutFacts {
  lines: CheckoutLine[];
  /** The lines that already have their register entry. */
  prescribed: readonly number[];
  hasBuyer: boolean;
  /** The ticket's customer file holds a NIN and an address (the register takes them). */
  customerIdentified: boolean;
}

export function checkoutSteps(facts: CheckoutFacts): CheckoutStep[] {
  const steps: CheckoutStep[] = [];
  facts.lines.forEach((l, line) => {
    if (l.productId && l.trackSerials && l.serials !== l.pieces) {
      steps.push({ kind: "serials", line, productId: l.productId, name: l.name, count: l.pieces });
    }
  });
  const controlled = facts.lines.flatMap((l, line) =>
    l.controlled && !facts.prescribed.includes(line) ? [{ line, name: l.name }] : [],
  );
  if (controlled.length > 0) steps.push({ kind: "prescriptions", lines: controlled });
  if (!facts.hasBuyer && !facts.customerIdentified && facts.lines.some((l) => l.jewel)) {
    steps.push({ kind: "buyer" });
  }
  return steps;
}
