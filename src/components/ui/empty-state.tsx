import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "./empty";

/**
 * The single empty/placeholder surface for lists and detail panes.
 *
 * Every empty state names what is missing and, where one exists, offers the
 * action that fills it — an empty screen should never be a dead end. Also used
 * for loading placeholders by passing a spinner icon.
 */
export function EmptyState({
  icon: Icon,
  iconClassName,
  title,
  description,
  action,
  className,
  ...props
}: {
  icon?: LucideIcon;
  iconClassName?: string;
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
} & Omit<React.ComponentProps<"div">, "children" | "title">) {
  return (
    <Empty
      data-slot="empty-state"
      className={cn("empty-state", className)}
      {...props}
    >
      <EmptyHeader>
        {Icon ? (
          <EmptyMedia variant="icon">
            <Icon className={iconClassName} aria-hidden="true" />
          </EmptyMedia>
        ) : null}
        <EmptyTitle>{title}</EmptyTitle>
        {description ? (
          <EmptyDescription>{description}</EmptyDescription>
        ) : null}
      </EmptyHeader>
      {action ? <EmptyContent>{action}</EmptyContent> : null}
    </Empty>
  );
}
