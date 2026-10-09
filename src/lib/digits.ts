const ARABIC_INDIC_ZERO = 0x0660;

/**
 * Arabic-Indic digits (U+0660–U+0669, typed on an Arabic keyboard) → ASCII 0–9; every other
 * character is kept. Phone, NIF and NIS inputs call it before submit, so numbers are stored and
 * printed with Western digits (and the backend's NIF/NIS check accepts ASCII digits only).
 */
export function toAsciiDigits(value: string): string {
  return value.replace(/[\u0660-\u0669]/g, (digit) => String(digit.charCodeAt(0) - ARABIC_INDIC_ZERO));
}
