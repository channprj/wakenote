import { Info } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";

type FieldHelpOpenListener = (openedId: string) => void;

const fieldHelpOpenListeners = new Set<FieldHelpOpenListener>();

export function announceFieldHelpOpen(openedId: string) {
  fieldHelpOpenListeners.forEach((listener) => listener(openedId));
}

export function subscribeToFieldHelpOpen(listener: FieldHelpOpenListener) {
  fieldHelpOpenListeners.add(listener);
  return () => {
    fieldHelpOpenListeners.delete(listener);
  };
}

export function FieldHelp({ label, description }: { label: string; description: string }) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLSpanElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const descriptionId = `field-help-${useId().replaceAll(":", "")}`;

  useEffect(
    () =>
      subscribeToFieldHelpOpen((openedId) => {
        if (openedId !== descriptionId) {
          setOpen(false);
        }
      }),
    [descriptionId],
  );

  useEffect(() => {
    if (!open) return;

    const closeOnOutsidePointer = (event: PointerEvent) => {
      const root = rootRef.current;
      if (root && event.target && !root.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        triggerRef.current?.focus();
      }
    };

    document.addEventListener("pointerdown", closeOnOutsidePointer);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  return (
    <span className="ui-field-help" ref={rootRef}>
      <button
        ref={triggerRef}
        type="button"
        className="ui-field-help__trigger"
        aria-label={`About ${label}`}
        aria-expanded={open}
        aria-controls={descriptionId}
        onClick={() => {
          if (!open) {
            announceFieldHelpOpen(descriptionId);
          }
          setOpen((current) => !current);
        }}
      >
        <Info aria-hidden="true" />
      </button>
      <span
        id={descriptionId}
        className="ui-field-help__popover"
        role="tooltip"
        hidden={!open}
      >
        {description}
      </span>
    </span>
  );
}
