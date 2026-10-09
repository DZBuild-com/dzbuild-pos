import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { errorKey, toAppError } from "@/lib/ipc/client";

/**
 * Translated "what happened + how to fix it" for any backend error. The error's `details`
 * object (when it is one) fills the message's placeholders (`{{pending}}`).
 */
export function useErrorText(): (error: unknown) => string {
  const { t, i18n } = useTranslation();
  return useCallback(
    (error: unknown) => {
      const details = toAppError(error).details;
      const values =
        typeof details === "object" && details !== null && !Array.isArray(details) ? details : {};
      return t(
        errorKey(error, (key) => i18n.exists(key)),
        values as Record<string, unknown>,
      );
    },
    [t, i18n],
  );
}

/** Translated message of a form field whose validation key is in `errors`, else undefined. */
export function useFieldError(
  errors: Partial<Record<string, string>>,
): (field: string) => string | undefined {
  const { t } = useTranslation();
  return (field: string) => {
    const key = errors[field];
    return key ? t(key) : undefined;
  };
}
