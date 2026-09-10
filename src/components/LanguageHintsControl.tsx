import { useState } from "react";
import { ChevronDown } from "lucide-react";
import { Button } from "./ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "./ui/dropdown-menu";
import { Field, FieldContent, FieldDescription, FieldLabel } from "./ui/field";
import { TRANSCRIPTION_LANGUAGE_OPTIONS } from "@/lib/transcription-languages";
import type { TranscriptionLanguage } from "@/lib/types";

export function LanguageHintsControl({
  value,
  onChange,
  label = "Transcription language hints",
  description = "Soniox can use several expected languages. Hints guide recognition without restricting it. Clear all hints for automatic detection.",
  disabled = false,
}: {
  value: readonly TranscriptionLanguage[];
  onChange: (languages: TranscriptionLanguage[]) => void | Promise<void>;
  label?: string;
  description?: string;
  disabled?: boolean;
}) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const selected = TRANSCRIPTION_LANGUAGE_OPTIONS.filter((option) =>
    value.includes(option.value),
  );
  const change = async (languages: TranscriptionLanguage[]) => {
    setSaving(true);
    setError(null);
    try {
      await onChange(languages);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSaving(false);
    }
  };
  return (
    <Field orientation="responsive" className="settings-row">
      <FieldContent>
        <FieldLabel>{label}</FieldLabel>
        <FieldDescription>{description}</FieldDescription>
        {error ? (
          <p role="alert" className="text-xs text-destructive">
            {error}
          </p>
        ) : null}
      </FieldContent>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            aria-label={label}
            disabled={disabled || saving}
            className="max-w-full min-w-0"
          >
            <span className="truncate">
              {selected.map((option) => option.label).join(", ") ||
                "Auto-detect"}
            </span>
            <ChevronDown className="shrink-0" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" aria-label={label}>
          {TRANSCRIPTION_LANGUAGE_OPTIONS.map((option) => (
            <DropdownMenuCheckboxItem
              key={option.value}
              checked={value.includes(option.value)}
              disabled={disabled || saving}
              onSelect={(event) => event.preventDefault()}
              onCheckedChange={(checked) =>
                void change(
                  checked
                    ? [...value, option.value]
                    : value.filter((language) => language !== option.value),
                )
              }
            >
              {option.label}
            </DropdownMenuCheckboxItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem
            disabled={disabled || saving || selected.length === 0}
            onSelect={() => void change([])}
          >
            Auto-detect (no hints)
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </Field>
  );
}
