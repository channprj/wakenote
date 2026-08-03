import { EyeIcon, EyeOffIcon } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";

export type ListVisibilityMode = "visible" | "hidden";

export interface ListVisibilityToolbarProps {
  mode: ListVisibilityMode;
  visibleCount: number;
  hiddenCount: number;
  selectedCount: number;
  totalInMode: number;
  mutating: boolean;
  statusMessage: string;
  onModeChange(mode: ListVisibilityMode): void;
  onSelectAll(): void;
  onClearSelection(): void;
  onApplySelection(): void;
}

export function ListVisibilityToolbar({
  mode,
  visibleCount,
  hiddenCount,
  selectedCount,
  totalInMode,
  mutating,
  statusMessage,
  onModeChange,
  onSelectAll,
  onClearSelection,
  onApplySelection,
}: ListVisibilityToolbarProps) {
  const allSelected =
    totalInMode > 0 && selectedCount === totalInMode;
  const checked =
    selectedCount === 0
      ? false
      : allSelected
        ? true
        : "indeterminate";
  const applyLabel =
    mode === "visible" ? "Hide selected" : "Restore selected";

  return (
    <div
      data-slot="list-visibility-toolbar"
      className="flex min-w-0 flex-wrap items-center justify-between gap-[var(--space-2)]"
    >
      <ToggleGroup
        type="single"
        value={mode}
        onValueChange={(value) => {
          if (value === "visible" || value === "hidden") {
            onModeChange(value);
          }
        }}
        aria-label="List visibility"
        className="bg-muted p-[3px]"
        spacing={0}
      >
        <ToggleGroupItem value="visible">
          <EyeIcon data-icon="inline-start" />
          Visible
          <Badge variant="outline">{visibleCount}</Badge>
        </ToggleGroupItem>
        <ToggleGroupItem value="hidden">
          <EyeOffIcon data-icon="inline-start" />
          Hidden
          <Badge variant="outline">{hiddenCount}</Badge>
        </ToggleGroupItem>
      </ToggleGroup>

      <div className="flex min-w-0 flex-wrap items-center gap-[var(--space-2)]">
        <label className="inline-flex min-h-[var(--control-compact)] items-center gap-[var(--space-2)] text-[length:var(--text-body)] leading-[var(--leading-body)] text-muted-foreground">
          <Checkbox
            checked={checked}
            disabled={mutating || totalInMode === 0}
            aria-label={allSelected ? "Clear selection" : "Select all items"}
            onCheckedChange={() => {
              if (allSelected) {
                onClearSelection();
              } else {
                onSelectAll();
              }
            }}
          />
          {selectedCount > 0
            ? `${selectedCount} selected`
            : "Select all"}
        </label>
        {selectedCount > 0 ? (
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={mutating}
            onClick={onClearSelection}
          >
            Clear
          </Button>
        ) : null}
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={mutating || selectedCount === 0}
          onClick={onApplySelection}
        >
          {applyLabel}
        </Button>
      </div>

      <span
        role="status"
        aria-live="polite"
        className="sr-only"
      >
        {statusMessage}
      </span>
    </div>
  );
}
