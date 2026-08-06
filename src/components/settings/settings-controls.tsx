import { Children, type ReactNode } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldTitle,
} from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { useMasonryGrid } from "@/hooks/use-masonry-grid";

export function SettingsCard({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <Card size="sm" className="settings-card">
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        {description ? <CardDescription>{description}</CardDescription> : null}
      </CardHeader>
      <CardContent>
        <FieldGroup className="gap-0 divide-y divide-border">
          {children}
        </FieldGroup>
      </CardContent>
    </Card>
  );
}

export function SettingSwitch({
  label,
  description,
  checked,
  disabled,
  title,
  onCheckedChange,
}: {
  label: string;
  description?: string;
  checked: boolean;
  disabled?: boolean;
  title?: string;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <Field
      orientation="horizontal"
      className="settings-row"
      data-disabled={disabled || undefined}
    >
      <FieldContent>
        <FieldTitle>{label}</FieldTitle>
        {description ? (
          <FieldDescription>{description}</FieldDescription>
        ) : null}
      </FieldContent>
      <Switch
        aria-label={label}
        checked={checked}
        disabled={disabled}
        title={title}
        onCheckedChange={onCheckedChange}
      />
    </Field>
  );
}

export interface SelectOption<T extends string = string> {
  value: T;
  label: string;
  group?: string;
}

export function SettingSelect<T extends string>({
  label,
  description,
  value,
  options,
  disabled,
  title,
  onValueChange,
}: {
  label: string;
  description?: string;
  value: T;
  options: ReadonlyArray<SelectOption<T>>;
  disabled?: boolean;
  title?: string;
  onValueChange: (value: T) => void;
}) {
  const optionGroups = options.reduce<
    Array<{ label: string | undefined; options: SelectOption<T>[] }>
  >((groups, option) => {
    const existing = groups.find((group) => group.label === option.group);
    if (existing) {
      existing.options.push(option);
    } else {
      groups.push({ label: option.group, options: [option] });
    }
    return groups;
  }, []);

  return (
    <Field
      orientation="responsive"
      className="settings-row"
      data-disabled={disabled || undefined}
    >
      <FieldContent>
        <FieldLabel>{label}</FieldLabel>
        {description ? (
          <FieldDescription>{description}</FieldDescription>
        ) : null}
      </FieldContent>
      <Select value={value} disabled={disabled} onValueChange={onValueChange}>
        <SelectTrigger
          size="sm"
          className="min-w-40"
          aria-label={label}
          title={title}
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {optionGroups.map((group) => (
            <SelectGroup key={group.label ?? "ungrouped"}>
              {group.label ? <SelectLabel>{group.label}</SelectLabel> : null}
              {group.options.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectGroup>
          ))}
        </SelectContent>
      </Select>
    </Field>
  );
}

export function SettingSlider({
  label,
  description,
  value,
  min,
  max,
  step = 1,
  suffix = "",
  disabled = false,
  valueLabel,
  onValueChange,
}: {
  label: string;
  description?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  suffix?: string;
  disabled?: boolean;
  valueLabel?: string;
  onValueChange: (value: number) => void;
}) {
  return (
    <Field
      orientation="responsive"
      className="settings-row settings-row--slider"
    >
      <FieldContent>
        <FieldLabel>{label}</FieldLabel>
        {description ? (
          <FieldDescription>{description}</FieldDescription>
        ) : null}
      </FieldContent>
      <div
        className={`settings-slider${valueLabel ? " settings-slider--wide-output" : ""}`}
      >
        <Slider
          aria-label={label}
          aria-disabled={disabled}
          value={[value]}
          min={min}
          max={max}
          step={step}
          disabled={disabled}
          onValueChange={(values) => onValueChange(values[0] ?? value)}
        />
        <output>{valueLabel ?? `${value}${suffix}`}</output>
      </div>
    </Field>
  );
}

export function SettingsGrid({
  children,
  maxColumns,
}: {
  children: ReactNode;
  maxColumns: 1 | 2 | 3;
}) {
  const itemCount = Children.toArray(children).length;
  const effectiveMaxColumns = Math.max(1, Math.min(itemCount, maxColumns));
  const gridRef = useMasonryGrid<HTMLDivElement>(itemCount);

  return (
    <div
      ref={gridRef}
      className="settings-grid"
      data-slot="settings-grid"
      data-max-columns={effectiveMaxColumns}
    >
      {children}
    </div>
  );
}
