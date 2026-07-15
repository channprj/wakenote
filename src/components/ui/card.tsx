import * as React from "react"

import { cn } from "@/lib/utils"

function Card({
  className,
  size = "default",
  ...props
}: React.ComponentProps<"div"> & { size?: "default" | "sm" }) {
  return (
    <div
      data-slot="card"
      data-size={size}
      className={cn(
        "group/card flex min-w-0 flex-col gap-(--card-spacing) overflow-hidden rounded-[var(--radius-card)] bg-card py-(--card-spacing) text-[length:var(--text-body)] leading-[var(--leading-body)] text-card-foreground ring-1 ring-foreground/10 [--card-spacing:var(--space-4)] has-data-[slot=card-footer]:pb-0 has-[>img:first-child]:pt-0 data-[size=sm]:[--card-spacing:var(--space-3)] data-[size=sm]:has-data-[slot=card-footer]:pb-0 *:[img:first-child]:rounded-t-[var(--radius-card)] *:[img:last-child]:rounded-b-[var(--radius-card)]",
        className
      )}
      {...props}
    />
  )
}

function CardHeader({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-header"
      className={cn(
        "group/card-header @container/card-header grid min-w-0 auto-rows-min items-start gap-[var(--space-1)] rounded-t-[var(--radius-card)] px-(--card-spacing) has-data-[slot=card-action]:grid-cols-[minmax(0,1fr)_auto] has-data-[slot=card-description]:grid-rows-[auto_auto] [.border-b]:pb-(--card-spacing)",
        className
      )}
      {...props}
    />
  )
}

function CardTitle({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-title"
      className={cn(
        "min-w-0 text-[length:var(--text-subtitle)] leading-[var(--leading-subtitle)] font-semibold group-data-[size=sm]/card:text-[length:var(--text-label)]",
        className
      )}
      {...props}
    />
  )
}

function CardDescription({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-description"
      className={cn(
        "min-w-0 text-[length:var(--text-body)] leading-[var(--leading-body)] text-muted-foreground",
        className,
      )}
      {...props}
    />
  )
}

function CardAction({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-action"
      className={cn(
        "col-start-2 row-span-2 row-start-1 min-w-0 self-start justify-self-end",
        className
      )}
      {...props}
    />
  )
}

function CardContent({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-content"
      className={cn("min-w-0 px-(--card-spacing)", className)}
      {...props}
    />
  )
}

function CardFooter({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-footer"
      className={cn(
        "flex min-w-0 flex-wrap items-center rounded-b-[var(--radius-card)] border-t bg-muted/50 p-(--card-spacing)",
        className
      )}
      {...props}
    />
  )
}

export {
  Card,
  CardHeader,
  CardFooter,
  CardTitle,
  CardAction,
  CardDescription,
  CardContent,
}
