import { X } from "lucide-react";
import { type KeyboardEvent, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useCustomer, useCustomerSearch } from "@/lib/ipc/customers";
import { normalizeDigits } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { cn } from "@/lib/utils";

/**
 * A document's customer (CRM-001): searched by name, phone or code (Arabic-Indic digits read as
 * 0-9), the matches listed under the field (arrows move, Enter takes the highlighted one), then
 * shown by name with a button that removes him.
 */
export function CustomerField({
  value,
  onChange,
  disabled = false,
}: {
  value: string | null;
  onChange: (id: string | null) => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const id = useId();
  const [text, setText] = useState("");
  const [active, setActive] = useState(0);
  const query = useDebounced(normalizeDigits(text.trim()), 150);
  const hits = useCustomerSearch({ text: query, limit: 8 }, value === null && query !== "");
  const chosen = useCustomer(value);
  const shown = text.trim() === "" ? [] : (hits.data ?? []);
  const pick = (customerId: string) => {
    setText("");
    setActive(0);
    onChange(customerId);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((a) => Math.max(0, Math.min(shown.length - 1, a + step)));
    } else if (event.key === "Enter") {
      const hit = shown[active] ?? shown[0];
      if (hit) {
        event.preventDefault();
        pick(hit.id);
      }
    } else if (event.key === "Escape" && text !== "") {
      event.stopPropagation();
      setText("");
    }
  };

  if (value !== null) {
    return (
      <div className="flex flex-col gap-1.5">
        <span id={`${id}-label`} className="font-semibold">
          {t("billing:editor.customer")}
        </span>
        <div
          role="group"
          aria-labelledby={`${id}-label`}
          className="inline-flex min-h-11 items-center gap-1 self-start rounded-full bg-brand-soft ps-3 font-semibold"
        >
          <span>{chosen.data?.name ?? "…"}</span>
          {chosen.data?.code ? (
            <bdi dir="ltr" className="text-sm font-normal text-ink-2">
              {chosen.data.code}
            </bdi>
          ) : null}
          <button
            type="button"
            disabled={disabled}
            aria-label={t("pos:customer.remove")}
            title={t("pos:customer.remove")}
            onClick={() => onChange(null)}
            className="grid size-11 place-items-center rounded-full text-ink-2 hover:bg-surface focus-visible:outline-3 focus-visible:outline-focus disabled:opacity-50"
          >
            <X aria-hidden className="size-4" />
          </button>
        </div>
      </div>
    );
  }
  return (
    <Field id={id} label={t("billing:editor.customer")} hint={t("pos:customer.searchHint")}>
      {/* the matches drop over the hint, under the field */}
      <div className="relative">
        <Input
          id={id}
          type="search"
          dir="auto"
          autoComplete="off"
          disabled={disabled}
          value={text}
          aria-controls={`${id}-hits`}
          aria-activedescendant={shown[active] ? `${id}-hit-${active}` : undefined}
          onChange={(event) => {
            setText(event.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
        />
        {shown.length > 0 ? (
          <ul
            id={`${id}-hits`}
            role="listbox"
            aria-label={t("pos:customer.results")}
            className="absolute inset-x-0 top-full z-20 mt-1 flex flex-col rounded-control border border-line bg-surface py-1 shadow-overlay"
          >
            {shown.map((hit, index) => (
              <li
                key={hit.id}
                id={`${id}-hit-${index}`}
                role="option"
                aria-selected={index === active}
                onPointerDown={(event) => event.preventDefault()}
                onClick={() => pick(hit.id)}
                className={cn(
                  "flex min-h-11 cursor-pointer items-center gap-2 px-3",
                  index === active ? "bg-brand-soft" : "hover:bg-surface-2",
                )}
              >
                <span className="truncate">{hit.name}</span>
                {hit.phone ? (
                  <bdi dir="ltr" className="text-sm text-ink-2">
                    {hit.phone}
                  </bdi>
                ) : null}
                <bdi dir="ltr" className="ms-auto text-sm text-ink-2">
                  {hit.code}
                </bdi>
              </li>
            ))}
          </ul>
        ) : null}
      </div>
    </Field>
  );
}
