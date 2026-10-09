import type { ReactNode } from "react";

/** Visible label above the control (never placeholder-only), inline error in --danger (spec §10.4). */
export function Field({
  id,
  label,
  optional,
  hint,
  error,
  children,
}: {
  id: string;
  label: string;
  optional?: string;
  hint?: string;
  error?: string | undefined;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="font-semibold">
        {label}
        {optional ? (
          <>
            {" "}
            <span className="font-normal text-ink-2">{optional}</span>
          </>
        ) : null}
      </label>
      {children}
      {hint ? <p className="text-sm text-ink-2">{hint}</p> : null}
      {error ? (
        <p role="alert" className="text-sm text-danger">
          {error}
        </p>
      ) : null}
    </div>
  );
}
