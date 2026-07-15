import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldTitle,
} from "./field";

export function FieldRow({
  label,
  description,
  help,
  control,
  className,
}: {
  label: ReactNode;
  description?: ReactNode;
  help?: ReactNode;
  control: ReactNode;
  className?: string;
}) {
  return (
    <div
      data-slot="field-row"
      className={cn(
        "grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-[var(--space-3)] py-[var(--space-2)]",
        className,
      )}
    >
      <Field orientation="horizontal" className="min-w-0">
        <FieldContent className="min-w-0 overflow-wrap-anywhere">
          <FieldTitle className="min-w-0 gap-1.5 text-[length:var(--text-label)] leading-[var(--leading-label)]">
            <span className="min-w-0">{label}</span>
            {help}
          </FieldTitle>
          {description ? (
            <FieldDescription className="min-w-0 text-[length:var(--text-caption)] leading-[var(--leading-caption)]">
              {description}
            </FieldDescription>
          ) : null}
        </FieldContent>
      </Field>
      <div className="shrink-0">{control}</div>
    </div>
  );
}
