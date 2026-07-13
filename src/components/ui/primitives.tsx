import { clsx } from "clsx";
import { useId } from "react";
import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from "react";
import type { StatusTone } from "../../lib/status-summary";
import { FieldHelp } from "./FieldHelp";

export function Button({
  className,
  variant = "primary",
  size = "md",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md" | "icon";
}) {
  return (
    <button
      className={clsx("ui-button", `ui-button--${variant}`, `ui-button--${size}`, className)}
      {...props}
    />
  );
}

export function Switch({
  checked,
  onCheckedChange,
  label,
  disabled = false,
  title,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      className="ui-switch"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      data-state={checked ? "checked" : "unchecked"}
      disabled={disabled}
      title={title}
      onClick={() => {
        if (!disabled) {
          onCheckedChange(!checked);
        }
      }}
    >
      <span />
    </button>
  );
}

export function Slider({
  label,
  value,
  min,
  max,
  step = 1,
  onValueChange,
  suffix,
  help,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onValueChange: (value: number) => void;
  suffix?: string;
  help?: string;
}) {
  const inputId = `slider-${useId().replaceAll(":", "")}`;
  const input = (
    <input
      id={inputId}
      className="ui-slider"
      type="range"
      value={value}
      min={min}
      max={max}
      step={step}
      onChange={(event) => onValueChange(Number(event.currentTarget.value))}
    />
  );

  if (!help) {
    return (
      <label className="ui-field">
        <span className="ui-field__label">
          <span>{label}</span>
          <strong>
            {value}
            {suffix}
          </strong>
        </span>
        {input}
      </label>
    );
  }

  return (
    <div className="ui-field">
      <span className="ui-field__label">
        <span className="ui-field__label-copy">
          <label htmlFor={inputId}>{label}</label>
          <FieldHelp label={label} description={help} />
        </span>
        <strong>
          {value}
          {suffix}
        </strong>
      </span>
      {input}
    </div>
  );
}

export function Select({
  label,
  children,
  ...props
}: SelectHTMLAttributes<HTMLSelectElement> & { label: string }) {
  return (
    <label className="ui-field">
      <span className="ui-field__label">{label}</span>
      <select className="ui-select" {...props}>
        {children}
      </select>
    </label>
  );
}

export function TextInput({
  label,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string }) {
  return (
    <label className="ui-field">
      <span className="ui-field__label">{label}</span>
      <input className="ui-input" {...props} />
    </label>
  );
}

export function TextArea({
  label,
  ...props
}: TextareaHTMLAttributes<HTMLTextAreaElement> & { label: string }) {
  return (
    <label className="ui-field">
      <span className="ui-field__label">{label}</span>
      <textarea className="ui-input ui-textarea" {...props} />
    </label>
  );
}

export function Badge({
  children,
  tone = "neutral",
}: {
  children: ReactNode;
  // Aligned with the canonical StatusTone union from status-summary.ts so that
  // future tone additions automatically reach this prop without a separate edit.
  tone?: StatusTone;
}) {
  return <span className={clsx("ui-badge", `ui-badge--${tone}`)}>{children}</span>;
}

export function Progress({ value }: { value: number }) {
  return (
    <div className="ui-progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={value}>
      <span style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />
    </div>
  );
}

export function Section({
  title,
  eyebrow,
  children,
  actions,
}: {
  title: string;
  eyebrow?: string;
  children: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <section className="section">
      <header className="section__header">
        <div>
          {eyebrow ? <p>{eyebrow}</p> : null}
          <h2>{title}</h2>
        </div>
        {actions ? <div className="section__actions">{actions}</div> : null}
      </header>
      {children}
    </section>
  );
}
