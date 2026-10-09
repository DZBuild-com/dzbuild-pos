import type { TFunction } from "i18next";
import { toAppError } from "@/lib/ipc/client";
import type { LabelWarning, LabelWarningCode } from "@/lib/ipc/types";

/** A label refusal as the user can act on it: a template too small for what it prints (its
 * field `template.widthDmm`/`heightDmm`, the designer's own) says where to make it larger;
 * any other error reads as usual. */
export function labelErrorText(error: unknown, t: TFunction, errorText: (error: unknown) => string): string {
  return toAppError(error).field?.startsWith("template.")
    ? t("labels:print.templateTooSmall")
    : errorText(error);
}

/** The products a warning names, each once. */
export function productsWith(warnings: readonly LabelWarning[], code: LabelWarningCode): number {
  return new Set(warnings.filter((w) => w.code === code).map((w) => w.productId)).size;
}
