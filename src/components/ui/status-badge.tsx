import type { ComponentProps } from "react";
import type { StatusTone } from "@/lib/status-summary";
import { cn } from "@/lib/utils";
import { Badge } from "./badge";

const toneVariant: Record<StatusTone, ComponentProps<typeof Badge>["variant"]> = {
  neutral: "outline",
  primary: "default",
  success: "outline",
  warning: "outline",
  danger: "destructive",
};

export function StatusBadge({
  tone,
  className,
  ...props
}: ComponentProps<typeof Badge> & { tone: StatusTone }) {
  return (
    <Badge
      data-tone={tone}
      variant={toneVariant[tone]}
      className={cn("max-w-full", className)}
      {...props}
    />
  );
}
