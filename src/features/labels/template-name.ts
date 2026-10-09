import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import type { LabelTemplate } from "@/lib/ipc/types";

const BUILTIN = "builtin.";

/** A template's name: its own, else a built-in's translated name (`labels:builtin.<suffix>`),
 * else its id (a shop template whose name was cleared). */
export function useTemplateName(): (template: LabelTemplate) => string {
  const { t, i18n } = useTranslation();
  return useCallback(
    (template: LabelTemplate) => {
      if (template.name.trim() !== "") return template.name;
      const key = `labels:builtin.${template.id.slice(BUILTIN.length)}`;
      return template.id.startsWith(BUILTIN) && i18n.exists(key) ? t(key) : template.id;
    },
    [t, i18n],
  );
}
