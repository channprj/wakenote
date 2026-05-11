import { clsx } from "clsx";
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, SelectHTMLAttributes } from "react";

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
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onValueChange: (value: number) => void;
  suffix?: string;
}) {
  return (
    <label className="ui-field">
      <span className="ui-field__label">
        {label}
        <strong>
          {value}
          {suffix}
        </strong>
      </span>
      <input
        className="ui-slider"
        type="range"
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(event) => onValueChange(Number(event.currentTarget.value))}
      />
    </label>
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

export function Badge({
  children,
  tone = "neutral",
}: {
  children: ReactNode;
  tone?: "neutral" | "success" | "warning" | "danger" | "primary";
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
