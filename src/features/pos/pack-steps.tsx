import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import type { BuyerIdentity, Module, PrescriptionInput } from "@/lib/ipc/types";
import { BuyerDialog } from "./BuyerDialog";
import { type FactLine, customerIdentified, lineFacts } from "./checkout-facts";
import { type CheckoutStep, checkoutSteps } from "./checkout-steps";
import { PrescriptionDialog } from "./PrescriptionDialog";
import { SerialDialog } from "./SerialDialog";

/** What the pack steps of a sale collected, by the line's index in the sale. */
export interface PackAnswers {
  serials: Record<number, string[]>;
  prescriptions: PrescriptionInput[];
  buyer: BuyerIdentity | null;
}

/**
 * The till's "Encaisser" steps (serials, controlled-medicine register entries, the jewel's buyer;
 * `checkout-steps.ts`) for a sale another screen makes (M8 final review: an instalment contract
 * signed on `/instalments/new` or from the till): one dialog each for what `lines` still lack,
 * then `onDone` with every answer; `onClose` when one is cancelled. Nothing to ask: `onDone` at
 * once.
 */
export function PackSteps({
  lines,
  customerId,
  modules,
  answers,
  onDone,
  onClose,
}: {
  lines: readonly Omit<FactLine, "serials">[];
  customerId: string | null;
  modules: readonly Module[];
  answers: PackAnswers;
  onDone: (answers: PackAnswers) => void;
  onClose: () => void;
}) {
  const client = useQueryClient();
  const [steps, setSteps] = useState<CheckoutStep[] | null>(null);
  const [state, setState] = useState<PackAnswers>(answers);
  // read once, when the steps open (the sale does not change behind its dialogs)
  const start = useRef({ lines, customerId, modules, answers });
  const finished = useRef(false);

  useEffect(() => {
    let live = true;
    const { lines: sold, customerId: customer, modules: on, answers: given } = start.current;
    void Promise.all([
      Promise.all(
        sold.map((line, index) =>
          lineFacts(client, on, { ...line, serials: given.serials[index]?.length ?? 0 }),
        ),
      ),
      customerIdentified(client, on, customer),
    ])
      .then(([facts, identified]) =>
        checkoutSteps({
          lines: facts,
          prescribed: given.prescriptions.map((p) => p.line),
          hasBuyer: given.buyer !== null,
          customerIdentified: identified,
        }),
      )
      .catch(() => [])
      .then((found) => {
        if (live) setSteps(found);
      });
    return () => {
      live = false;
    };
  }, [client]);

  useEffect(() => {
    if (steps?.length === 0 && !finished.current) {
      finished.current = true;
      onDone(state);
    }
  }, [steps, state, onDone]);

  const step = steps?.[0];
  if (!step) return null;
  const next = (change: (s: PackAnswers) => PackAnswers) => {
    setState(change);
    setSteps((all) => (all ? all.slice(1) : all));
  };
  switch (step.kind) {
    case "serials":
      return (
        <SerialDialog
          key={`serials-${step.line}`}
          productId={step.productId}
          name={step.name}
          count={step.count}
          initial={state.serials[step.line] ?? []}
          onDone={(list) => next((s) => ({ ...s, serials: { ...s.serials, [step.line]: list } }))}
          onClose={onClose}
        />
      );
    case "prescriptions":
      return (
        <PrescriptionDialog
          lines={step.lines}
          initial={state.prescriptions}
          onDone={(given) =>
            next((s) => ({
              ...s,
              prescriptions: [
                ...s.prescriptions.filter((p) => !given.some((g) => g.line === p.line)),
                ...given,
              ],
            }))
          }
          onClose={onClose}
        />
      );
    case "buyer":
      return (
        <BuyerDialog
          initial={state.buyer}
          onDone={(buyer) => next((s) => ({ ...s, buyer }))}
          onClose={onClose}
        />
      );
  }
}
