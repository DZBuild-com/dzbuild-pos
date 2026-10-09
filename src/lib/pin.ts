export const PIN_PATTERN = /^\d{4,8}$/;

/** Same rule as pos-core `is_too_simple`: all digits equal, or a straight run up or down. */
export function isTooSimplePin(pin: string): boolean {
  const digits = [...pin].map(Number);
  const pairs = digits.slice(1).map((digit, index) => [digits[index] ?? 0, digit] as const);
  return (
    pairs.every(([a, b]) => a === b) ||
    pairs.every(([a, b]) => b - a === 1) ||
    pairs.every(([a, b]) => a - b === 1)
  );
}

/** Validation keys for a new PIN and its confirmation; the only copy of these form rules. */
export function pinErrors(pin: string, confirm: string): Partial<Record<"pin" | "pinConfirm", string>> {
  const errors: Partial<Record<"pin" | "pinConfirm", string>> = {};
  if (!PIN_PATTERN.test(pin)) errors.pin = "validation:pinFormat";
  else if (isTooSimplePin(pin)) errors.pin = "validation:pinTooSimple";
  if (pin !== confirm) errors.pinConfirm = "validation:pinMismatch";
  return errors;
}
